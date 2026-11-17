//! Shared metadata transactions and immutable asset ownership for all adapters.
mod analysis;
mod commands;
mod project;
mod queries;
mod types;

use super::arrangement::validate_identifier as identifier;
use super::arrangement::{Arrangement, ArrangementHistory, AssetLibrary, RenderOptions};
use anyhow::{Result, ensure};
pub use commands::edit_label;
use std::collections::BTreeMap;
pub use types::{
    AssetInfo, EqResponse, EqTarget, HistoryInfo, OwnedAsset, SessionLimits, SessionSnapshot,
    Waveform,
};

#[derive(Clone)]
pub struct ArrangementSession {
    pub(super) history: ArrangementHistory,
    pub(super) assets: AssetLibrary,
    pub(super) catalog: BTreeMap<String, AssetInfo>,
    pub(super) revision: u64,
    pub(super) epoch: String,
    pub(super) before: Arrangement,
    pub(super) origin: Arrangement,
    pub(super) undo_labels: Vec<String>,
    pub(super) redo_labels: Vec<String>,
    pub(super) options: RenderOptions,
    pub(super) limits: SessionLimits,
}

impl ArrangementSession {
    pub fn new(
        sample_rate: u32,
        channels: usize,
        epoch: String,
        limits: SessionLimits,
    ) -> Result<Self> {
        identifier(&epoch)?;
        ensure!(
            limits.max_decoded_bytes > 0,
            "Arrangement decoded budget must be positive"
        );
        let assets = AssetLibrary::new();
        let document = Arrangement::new(sample_rate, channels);
        let history = ArrangementHistory::new(document.clone(), &assets, limits.history_capacity)?;
        Ok(Self {
            history,
            assets,
            catalog: BTreeMap::new(),
            revision: 0,
            epoch,
            before: document.clone(),
            origin: document,
            undo_labels: vec![],
            redo_labels: vec![],
            options: RenderOptions::default(),
            limits,
        })
    }
    pub fn document(&self) -> &Arrangement {
        self.history.current()
    }
    pub fn before(&self) -> &Arrangement {
        &self.before
    }
    pub fn origin(&self) -> &Arrangement {
        &self.origin
    }
    pub fn assets(&self) -> &AssetLibrary {
        &self.assets
    }
    pub fn render_options(&self) -> &RenderOptions {
        &self.options
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn epoch(&self) -> &str {
        &self.epoch
    }
    pub fn read(&self) -> SessionSnapshot {
        SessionSnapshot {
            epoch: self.epoch.clone(),
            revision: self.revision,
            arrangement: self.document().clone(),
            assets: self.catalog.values().cloned().collect(),
            history: HistoryInfo {
                can_undo: self.history.can_undo(),
                can_redo: self.history.can_redo(),
                undo_label: self.undo_labels.last().cloned(),
                redo_label: self.redo_labels.last().cloned(),
            },
            render_options: self.options.clone(),
        }
    }
    pub fn check_revision(&self, expected: u64) -> Result<()> {
        ensure!(
            expected == self.revision,
            "Arrangement revision is stale; refresh the snapshot"
        );
        ensure!(
            self.revision < 9_007_199_254_740_991,
            "Arrangement revision budget exhausted"
        );
        Ok(())
    }
}

fn label(value: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control),
        "Arrangement label must be nonempty bounded plain text"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
