use super::{ArrangementSnapshot, plain_label};
use crate::{
    source_identity::SourceIdentity,
    studio::arrangement::{
        Arrangement, AssetLibrary, DEFAULT_HISTORY_METADATA_BYTES, HistorySnapshot,
        MAX_METADATA_BYTES, RenderOptions, serialized_bytes, validate_identifier,
    },
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_PROJECT_MANIFEST_BYTES: usize =
    DEFAULT_HISTORY_METADATA_BYTES + 3 * MAX_METADATA_BYTES + 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSource {
    pub id: String,
    pub name: String,
    pub file: String,
    pub identity: SourceIdentity,
    pub frames: u64,
    pub sample_rate: u32,
    pub channels: usize,
}
impl ProjectSource {
    pub fn validate(&self) -> Result<()> {
        validate_identifier(&self.id)?;
        plain_label(&self.name)?;
        ensure!(
            self.identity.sha256.len() == 64
                && self
                    .identity
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                && self.file == format!("assets/{}.wav", self.identity.sha256),
            "Project sources must use canonical hash-named relative WAV files"
        );
        ensure!(
            (8000..=384000).contains(&self.sample_rate)
                && (1..=2).contains(&self.channels)
                && self.frames > 0,
            "Invalid project source geometry"
        );
        let bytes = self
            .frames
            .checked_mul(self.channels as u64)
            .and_then(|samples| samples.checked_mul(4))
            .and_then(|bytes| bytes.checked_add(4096));
        ensure!(
            self.identity.bytes >= 44
                && bytes.is_some_and(|maximum| self.identity.bytes <= maximum),
            "Project WAV size does not match its declared float PCM geometry"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArrangementManifest {
    pub format: String,
    pub version: u32,
    pub history: HistorySnapshot,
    pub sources: Vec<ProjectSource>,
    pub render_options: RenderOptions,
    #[serde(default)]
    pub origin: Option<Arrangement>,
    #[serde(default)]
    pub before: Option<Arrangement>,
    #[serde(default)]
    pub undo_labels: Vec<String>,
    #[serde(default)]
    pub redo_labels: Vec<String>,
}
impl ArrangementManifest {
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_PROJECT_MANIFEST_BYTES,
            "Project manifest exceeds the metadata budget"
        );
        let value: Self = serde_json::from_slice(bytes)?;
        value.validate()?;
        Ok(value)
    }
    pub fn to_json(&self) -> Result<Vec<u8>> {
        self.validate()?;
        serialized_bytes(self, MAX_PROJECT_MANIFEST_BYTES)?;
        Ok(serde_json::to_vec(self)?)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.format == "slimeybeats-studio" && self.version == 2,
            "Unsupported arrangement project version"
        );
        self.render_options.validate()?;
        ensure!(
            (1..=64).contains(&self.history.capacity)
                && self.history.undo.len() <= self.history.capacity
                && self.history.redo.len() <= self.history.capacity,
            "Project history has invalid retained-state counts"
        );
        serialized_bytes(&self.history, DEFAULT_HISTORY_METADATA_BYTES)?;
        for document in self
            .history
            .undo
            .iter()
            .chain(&self.history.redo)
            .chain(std::iter::once(&self.history.current))
            .chain(self.origin.iter())
            .chain(self.before.iter())
        {
            serialized_bytes(document, MAX_METADATA_BYTES)?;
        }
        serialized_bytes(&self.sources, MAX_METADATA_BYTES)?;
        let mut ids = std::collections::BTreeSet::new();
        for source in &self.sources {
            source.validate()?;
            ensure!(ids.insert(&source.id), "Duplicate project source ID");
        }
        Ok(())
    }
    pub(super) fn restore(self, assets: AssetLibrary) -> Result<ArrangementSnapshot> {
        self.validate()?;
        ensure!(
            assets.len() == self.sources.len(),
            "Project source pool is incomplete"
        );
        let names: BTreeMap<_, _> = self
            .sources
            .iter()
            .map(|source| (source.id.clone(), source.name.clone()))
            .collect();
        for source in &self.sources {
            let audio = assets
                .get(&source.id)
                .ok_or_else(|| anyhow::anyhow!("Project source ID is missing"))?;
            ensure!(
                audio.frames() as u64 == source.frames
                    && audio.sample_rate == source.sample_rate
                    && audio.channels == source.channels,
                "Loaded project source geometry differs from manifest"
            );
        }
        let value = ArrangementSnapshot {
            history: self.history,
            assets,
            asset_names: names,
            render_options: self.render_options,
            origin: self.origin,
            before: self.before,
            undo_labels: self.undo_labels,
            redo_labels: self.redo_labels,
        };
        value.validate()?;
        Ok(value)
    }
}
