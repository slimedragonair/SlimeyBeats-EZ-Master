//! Native disk adapter for the same v2 manifest used by browser ZIPs and MCP.
pub use crate::studio::arrangement_project::ArrangementSnapshot;
use crate::{
    source_identity::SourceIdentity,
    studio::arrangement_project::{
        ArrangementManifest, MAX_PROJECT_MANIFEST_BYTES, ProjectLoader, ProjectSource,
    },
    wav_encoding,
};
use anyhow::{Context, Result, ensure};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MANIFEST: &str = "project.json";
#[derive(Clone, Copy)]
pub struct ProjectLimits {
    pub max_asset_bytes: usize,
    pub max_total_bytes: u64,
    pub max_output_bytes: u64,
}
impl Default for ProjectLimits {
    fn default() -> Self {
        Self {
            max_asset_bytes: super::stem_files::DEFAULT_INPUT_DECODED_BYTES,
            max_total_bytes: super::stem_files::DEFAULT_BANK_DECODED_BYTES,
            max_output_bytes: 512 * 1024 * 1024,
        }
    }
}
impl ProjectLimits {
    fn validate(self) -> Result<()> {
        ensure!(
            self.max_asset_bytes > 0
                && self.max_asset_bytes <= isize::MAX as usize
                && self.max_total_bytes > 0
                && self.max_output_bytes > 0,
            "Project memory budgets must be positive and supported"
        );
        Ok(())
    }
}
pub enum LoadedStudioProject {
    Legacy(Box<super::session::StudioSnapshot>),
    Arrangement(Box<ArrangementSnapshot>),
}
pub fn save(
    snapshot: &ArrangementSnapshot,
    destination: &Path,
    cancel: &dyn Fn() -> bool,
) -> Result<PathBuf> {
    save_with_limits(snapshot, destination, ProjectLimits::default(), cancel)
}
pub fn save_with_limits(
    snapshot: &ArrangementSnapshot,
    destination: &Path,
    limits: ProjectLimits,
    cancel: &dyn Fn() -> bool,
) -> Result<PathBuf> {
    limits.validate()?;
    ensure!(!cancel(), "Cancelled");
    snapshot.validate()?;
    ensure!(
        snapshot.decoded_bytes()? <= limits.max_total_bytes,
        "Project sources exceed the total decoded budget"
    );
    std::fs::create_dir_all(destination)?;
    let stage = tempfile::Builder::new()
        .prefix("Studio-arrangement-")
        .tempdir_in(destination)?;
    let assets_directory = stage.path().join("assets");
    std::fs::create_dir(&assets_directory)?;
    let mut sources = Vec::new();
    for (id, audio) in &snapshot.assets {
        ensure!(!cancel(), "Cancelled");
        ensure!(
            (audio.samples.len() as u64) * 4 <= limits.max_asset_bytes as u64,
            "Project source exceeds the per-input decoded budget"
        );
        let temporary = tempfile::Builder::new()
            .prefix("source-")
            .suffix(".wav")
            .tempfile_in(&assets_directory)?;
        wav_encoding::write_wav(audio, temporary.as_file(), "wav32f", false)?;
        temporary.as_file().sync_all()?;
        let identity = SourceIdentity::read(temporary.path(), cancel)?;
        let file = format!("assets/{}.wav", identity.sha256);
        let destination = stage.path().join(&file);
        if destination.exists() {
            ensure!(
                SourceIdentity::read(&destination, cancel)? == identity,
                "Conflicting project source payload"
            );
        } else {
            temporary
                .persist_noclobber(&destination)
                .map_err(|error| error.error)?;
        }
        sources.push(ProjectSource {
            id: id.clone(),
            name: snapshot
                .asset_names
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.clone()),
            file,
            identity,
            frames: audio.frames() as u64,
            sample_rate: audio.sample_rate,
            channels: audio.channels,
        });
    }
    let bytes = serde_json::to_vec_pretty(&snapshot.manifest(sources)?)?;
    ensure!(
        bytes.len() <= MAX_PROJECT_MANIFEST_BYTES,
        "Project manifest exceeds the metadata budget"
    );
    ensure!(!cancel(), "Cancelled");
    let mut output = std::fs::File::create_new(stage.path().join(MANIFEST))?;
    output.write_all(&bytes)?;
    output.sync_all()?;
    ensure!(!cancel(), "Cancelled");
    Ok(stage.keep().join(MANIFEST))
}
fn read_manifest(path: &Path, cancel: &dyn Fn() -> bool) -> Result<Vec<u8>> {
    ensure!(!cancel(), "Cancelled");
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= MAX_PROJECT_MANIFEST_BYTES as u64,
        "Project manifest must be an ordinary file within the metadata budget"
    );
    let bytes = std::fs::read(path)?;
    ensure!(!cancel(), "Cancelled");
    Ok(bytes)
}
pub fn load(
    path: &Path,
    max_decoded_bytes: u64,
    cancel: &dyn Fn() -> bool,
) -> Result<ArrangementSnapshot> {
    let defaults = ProjectLimits::default();
    let limits = ProjectLimits {
        max_total_bytes: max_decoded_bytes,
        max_asset_bytes: defaults
            .max_asset_bytes
            .min(max_decoded_bytes.min(usize::MAX as u64) as usize),
        max_output_bytes: defaults.max_output_bytes.min(max_decoded_bytes),
    };
    load_with_limits(path, limits, cancel)
}
pub fn load_with_limits(
    path: &Path,
    limits: ProjectLimits,
    cancel: &dyn Fn() -> bool,
) -> Result<ArrangementSnapshot> {
    limits.validate()?;
    let manifest = ArrangementManifest::from_json(&read_manifest(path, cancel)?)?;
    let root = path
        .parent()
        .context("Project directory is missing")?
        .canonicalize()?;
    let mut loader = ProjectLoader::new(manifest, limits.max_total_bytes)?;
    for source in loader.sources().to_vec() {
        ensure!(!cancel(), "Cancelled");
        source.validate()?;
        let declared = source
            .frames
            .checked_mul(source.channels as u64)
            .and_then(|n| n.checked_mul(4))
            .context("Project source size overflow")?;
        ensure!(
            declared <= limits.max_asset_bytes as u64 && declared <= loader.remaining_bytes(),
            "Project source exceeds the remaining decoded budget"
        );
        let source_path = root.join(&source.file);
        let metadata = std::fs::symlink_metadata(&source_path)?;
        ensure!(
            metadata.file_type().is_file() && metadata.len() == source.identity.bytes,
            "Project source must be an ordinary file with the recorded size"
        );
        let canonical = source_path.canonicalize()?;
        ensure!(
            canonical.starts_with(&root),
            "Project source escaped its directory"
        );
        let mut input = std::fs::File::open(canonical)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(usize::try_from(source.identity.bytes)?)
            .context("Cannot allocate project source input")?;
        let mut block = [0_u8; 65536];
        loop {
            ensure!(!cancel(), "Cancelled");
            let count = input.read(&mut block)?;
            if count == 0 {
                break;
            }
            ensure!(
                bytes.len() as u64 + count as u64 <= source.identity.bytes,
                "Project source grew while loading"
            );
            bytes.extend_from_slice(&block[..count]);
        }
        loader.add_source(&source.id, &bytes)?;
    }
    ensure!(!cancel(), "Cancelled");
    let mut snapshot = loader.finish()?;
    // Resource budgets are local policy, not trusted settings from a shared project.
    snapshot.render_options.max_output_bytes = snapshot
        .render_options
        .max_output_bytes
        .min(limits.max_output_bytes);
    Ok(snapshot)
}
pub fn load_any(
    path: &Path,
    max_decoded_bytes: u64,
    cancel: &dyn Fn() -> bool,
) -> Result<LoadedStudioProject> {
    load_any_with_limits(
        path,
        ProjectLimits {
            max_total_bytes: max_decoded_bytes,
            max_asset_bytes: ProjectLimits::default()
                .max_asset_bytes
                .min(max_decoded_bytes.min(usize::MAX as u64) as usize),
            max_output_bytes: ProjectLimits::default()
                .max_output_bytes
                .min(max_decoded_bytes),
        },
        cancel,
    )
}
pub fn load_any_with_limits(
    path: &Path,
    limits: ProjectLimits,
    cancel: &dyn Fn() -> bool,
) -> Result<LoadedStudioProject> {
    limits.validate()?;
    let header: serde_json::Value = serde_json::from_slice(&read_manifest(path, cancel)?)?;
    match header.get("version").and_then(serde_json::Value::as_u64) {
        Some(2) => Ok(LoadedStudioProject::Arrangement(Box::new(
            load_with_limits(path, limits, cancel)?,
        ))),
        Some(1) => {
            ensure!(!cancel(), "Cancelled");
            Ok(LoadedStudioProject::Legacy(Box::new(
                super::session::StudioDocument::load(path)?,
            )))
        }
        _ => anyhow::bail!("Unsupported Studio project version"),
    }
}
#[cfg(test)]
#[path = "arrangement_project_tests.rs"]
mod tests;
