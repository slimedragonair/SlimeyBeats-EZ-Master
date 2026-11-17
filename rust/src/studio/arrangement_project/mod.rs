//! Canonical native-directory/web-ZIP project metadata and immutable source pool.
mod archive;
mod loader;
mod manifest;
mod sources;
pub use archive::load_zip;
pub use loader::ProjectLoader;
pub use manifest::{ArrangementManifest, MAX_PROJECT_MANIFEST_BYTES, ProjectSource};
pub use sources::{decode_source, describe_source, encode_source};

use super::arrangement::{
    Arrangement, ArrangementHistory, AssetLibrary, HistorySnapshot, RenderOptions, check_metadata,
    validate_identifier,
};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

pub struct ArrangementSnapshot {
    pub history: HistorySnapshot,
    pub assets: AssetLibrary,
    pub asset_names: BTreeMap<String, String>,
    pub render_options: RenderOptions,
    pub origin: Option<Arrangement>,
    pub before: Option<Arrangement>,
    pub undo_labels: Vec<String>,
    pub redo_labels: Vec<String>,
}

impl ArrangementSnapshot {
    pub fn new(
        history: HistorySnapshot,
        assets: AssetLibrary,
        asset_names: BTreeMap<String, String>,
        render_options: RenderOptions,
    ) -> Result<Self> {
        let history = ArrangementHistory::from_snapshot(history, &assets)?.snapshot();
        let value = Self {
            history,
            assets,
            asset_names,
            render_options,
            origin: None,
            before: None,
            undo_labels: vec![],
            redo_labels: vec![],
        };
        value.validate()?;
        Ok(value)
    }
    pub fn document(&self) -> &Arrangement {
        &self.history.current
    }
    pub fn decoded_bytes(&self) -> Result<u64> {
        self.assets.values().try_fold(0_u64, |total, audio| {
            total
                .checked_add(audio.samples.len() as u64 * 4)
                .context("Project source size overflow")
        })
    }
    pub fn validate(&self) -> Result<()> {
        self.render_options.validate()?;
        for (id, audio) in &self.assets {
            validate_identifier(id)?;
            ensure!(
                audio.sample_rate == self.document().sample_rate
                    && (audio.channels == self.document().channels
                        || (audio.channels == 1 && self.document().channels == 2)),
                "Project source layout differs; conform explicitly first"
            );
            ensure!(
                !audio.samples.is_empty()
                    && (1..=2).contains(&audio.channels)
                    && audio.samples.len().is_multiple_of(audio.channels)
                    && audio.samples.iter().all(|sample| sample.is_finite()),
                "Project source PCM is invalid"
            );
            if let Some(name) = self.asset_names.get(id) {
                plain_label(name)?;
            }
        }
        ensure!(
            self.asset_names
                .keys()
                .all(|id| self.assets.contains_key(id)),
            "Project names refer to an unknown source ID"
        );
        for document in self
            .history
            .undo
            .iter()
            .chain(&self.history.redo)
            .chain(std::iter::once(self.document()))
        {
            document.validate(&self.assets)?;
        }
        for document in [&self.origin, &self.before].into_iter().flatten() {
            document.validate(&self.assets)?;
        }
        check_metadata(&self.asset_names)?;
        for labels in [&self.undo_labels, &self.redo_labels] {
            for label in labels {
                plain_label(label)?;
            }
        }
        ensure!(
            (self.undo_labels.is_empty() || self.undo_labels.len() == self.history.undo.len())
                && (self.redo_labels.is_empty()
                    || self.redo_labels.len() == self.history.redo.len()),
            "Project history labels are not aligned with retained states"
        );
        Ok(())
    }
    pub fn manifest(&self, sources: Vec<ProjectSource>) -> Result<ArrangementManifest> {
        self.validate()?;
        ensure!(
            sources.len() == self.assets.len(),
            "Project source pool is incomplete"
        );
        let mut ids = std::collections::BTreeSet::new();
        for source in &sources {
            source.validate()?;
            ensure!(
                ids.insert(source.id.as_str()),
                "Duplicate project source ID"
            );
            let audio = self
                .assets
                .get(&source.id)
                .context("Project source ID is unknown")?;
            ensure!(
                source.frames == audio.frames() as u64
                    && source.sample_rate == audio.sample_rate
                    && source.channels == audio.channels,
                "Project source descriptor has wrong frame geometry"
            );
        }
        let value = ArrangementManifest {
            format: "slimeybeats-studio".into(),
            version: 2,
            history: self.history.clone(),
            sources,
            render_options: self.render_options.clone(),
            origin: self.origin.clone(),
            before: self.before.clone(),
            undo_labels: self.undo_labels.clone(),
            redo_labels: self.redo_labels.clone(),
        };
        value.to_json()?;
        Ok(value)
    }
}

fn plain_label(value: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control),
        "Project labels must be bounded plain text"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
