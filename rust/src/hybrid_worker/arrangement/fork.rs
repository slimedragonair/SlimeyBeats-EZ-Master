//! Private job IPC only. User-facing projects retain the canonical v2 format.
use super::workspace::Workspace;
use crate::hybrid_worker::{input, paths::no_links};
use anyhow::{Context, Result, ensure};
use mastercraft::{
    audio::Audio,
    source_identity::SourceIdentity,
    studio::{
        arrangement::{
            Arrangement, AssetLibrary, AssetValidationCache, HistorySnapshot, RenderOptions,
        },
        arrangement_project::{ArrangementSnapshot, MAX_PROJECT_MANIFEST_BYTES},
        arrangement_session::{ArrangementSession, SessionLimits},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceFile {
    id: String,
    name: String,
    file: PathBuf,
    identity: SourceIdentity,
    frames: u64,
    sample_rate: u32,
    channels: usize,
}
impl SourceFile {
    pub fn new(
        id: String,
        name: String,
        file: PathBuf,
        identity: SourceIdentity,
        audio: &Audio,
    ) -> Self {
        Self {
            id,
            name,
            file,
            identity,
            frames: audio.frames() as u64,
            sample_rate: audio.sample_rate,
            channels: audio.channels,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transfer {
    version: u32,
    epoch: String,
    revision: u64,
    history: HistorySnapshot,
    render_options: RenderOptions,
    origin: Option<Arrangement>,
    before: Option<Arrangement>,
    undo_labels: Vec<String>,
    redo_labels: Vec<String>,
    sources: Vec<SourceFile>,
}

impl Workspace {
    pub(super) fn job_directory(&self, id: &str) -> Result<PathBuf> {
        ensure!(
            id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "Use an opaque job ID"
        );
        self.paths
            .directory(&self.paths.root.join(format!("arrangement-job-{id}")))
    }
    pub(super) fn fork_state(
        &mut self,
        revision: u64,
        id: &str,
        max_bytes: u64,
    ) -> Result<serde_json::Value> {
        self.selected(revision)?;
        let snapshot = self.session()?.project_snapshot()?;
        let folder = self.job_directory(id)?;
        // Normal admission already retains immutable original files. A project
        // opened from ZIP materializes missing sources once in this explicit job.
        for (index, (asset_id, audio)) in snapshot.assets.iter().enumerate() {
            if self.sources.contains_key(asset_id) {
                continue;
            }
            let file = folder.join(format!("source-{index}.wav"));
            let mut output = std::fs::File::create_new(&file)?;
            mastercraft::wav_encoding::write_wav(audio, &mut output, "wav32f", false)?;
            output.flush()?;
            output.sync_all()?;
            let identity = SourceIdentity::read(&file, &|| false)?;
            self.sources.insert(
                asset_id.clone(),
                SourceFile::new(
                    asset_id.clone(),
                    snapshot.asset_names[asset_id].clone(),
                    file,
                    identity,
                    audio,
                ),
            );
        }
        let transfer = Transfer {
            version: 1,
            epoch: self.session()?.epoch().into(),
            revision,
            history: snapshot.history,
            render_options: snapshot.render_options,
            origin: snapshot.origin,
            before: snapshot.before,
            undo_labels: snapshot.undo_labels,
            redo_labels: snapshot.redo_labels,
            sources: self.sources.values().cloned().collect(),
        };
        let bytes = serde_json::to_vec(&transfer)?;
        ensure!(
            bytes.len() as u64
                <= max_bytes.min((MAX_PROJECT_MANIFEST_BYTES + 16 * 1024 * 1024) as u64),
            "Private job state exceeds its metadata budget"
        );
        let file = folder.join("transfer.json");
        let mut output = std::fs::File::create_new(&file)?;
        output.write_all(&bytes)?;
        output.sync_all()?;
        Ok(serde_json::json!({"file":file,"revision":revision,"epoch":transfer.epoch}))
    }
    pub(super) fn restore_fork(&mut self, file: &Path) -> Result<serde_json::Value> {
        no_links(file)?;
        let file = self.paths.input(file)?;
        let maximum = MAX_PROJECT_MANIFEST_BYTES + 16 * 1024 * 1024;
        ensure!(
            file.metadata()?.len() <= maximum as u64,
            "Private job state exceeds its metadata budget"
        );
        let mut bytes = Vec::new();
        std::fs::File::open(file)?
            .take(maximum as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= maximum,
            "Private job state exceeds its metadata budget"
        );
        let transfer: Transfer = serde_json::from_slice(&bytes)?;
        ensure!(transfer.version == 1, "Unsupported private job transfer");
        let mut assets = AssetLibrary::new();
        let mut names = BTreeMap::new();
        let mut sources = BTreeMap::new();
        let mut remaining = self.limits.max_bank_pcm_bytes;
        for source in transfer.sources {
            no_links(&source.file)?;
            let path = self.paths.input(&source.file)?;
            let (audio, identity) = input::load_identified(&path, &self.limits, remaining)?;
            ensure!(
                identity == source.identity
                    && audio.frames() as u64 == source.frames
                    && audio.sample_rate == source.sample_rate
                    && audio.channels == source.channels,
                "Private source changed before job admission"
            );
            remaining = remaining
                .checked_sub(audio.samples.len() * 4)
                .context("Job source pool exceeds its budget")?;
            ensure!(
                assets.insert(source.id.clone(), Arc::new(audio)).is_none(),
                "Duplicate private source ID"
            );
            names.insert(source.id.clone(), source.name.clone());
            sources.insert(source.id.clone(), source);
        }
        let snapshot = ArrangementSnapshot {
            history: transfer.history,
            assets,
            asset_names: names,
            render_options: transfer.render_options,
            origin: transfer.origin,
            before: transfer.before,
            undo_labels: transfer.undo_labels,
            redo_labels: transfer.redo_labels,
        };
        let session = ArrangementSession::from_project_at_revision(
            snapshot,
            transfer.epoch,
            SessionLimits {
                max_decoded_bytes: self.limits.max_bank_pcm_bytes as u64,
                ..Default::default()
            },
            transfer.revision,
        )?;
        self.session = Some(session);
        self.sources = sources;
        self.player = None;
        self.prepared_revision = None;
        self.cache = AssetValidationCache::new();
        self.snapshot()
    }
    pub(super) fn save_project(&self, revision: u64, id: &str) -> Result<serde_json::Value> {
        let snapshot = self.selected(revision)?.project_snapshot()?;
        let folder = self.job_directory(id)?.join("project");
        let manifest = mastercraft::studio_native::arrangement_project::save_with_limits(
            &snapshot,
            &folder,
            mastercraft::studio_native::arrangement_project::ProjectLimits {
                max_asset_bytes: self.limits.max_pcm_bytes,
                max_total_bytes: self.limits.max_bank_pcm_bytes as u64,
                max_output_bytes: self.session()?.render_options().max_output_bytes,
            },
            &|| false,
        )?;
        Ok(serde_json::json!({"file":manifest,"revision":revision}))
    }
}
