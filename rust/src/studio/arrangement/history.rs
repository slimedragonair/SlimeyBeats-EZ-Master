use super::{Arrangement, AssetLibrary, Edit, apply_edits};
use anyhow::{Context, Result, ensure};

#[path = "history_snapshot.rs"]
mod snapshot;
pub use snapshot::HistorySnapshot;

#[path = "history_budget.rs"]
mod retention;
pub use retention::DEFAULT_HISTORY_METADATA_BYTES;
use retention::{RetainedState, envelope_bytes, state_bytes};

/// Metadata snapshots only. Callers retain assets referenced by every history state.
#[derive(Clone)]
pub struct ArrangementHistory {
    current: Arrangement,
    undo: Vec<RetainedState>,
    redo: Vec<RetainedState>,
    capacity: usize,
    current_bytes: usize,
    state_bytes: usize,
    max_serialized_bytes: usize,
    sequence: u64,
}
impl ArrangementHistory {
    pub fn new(initial: Arrangement, assets: &AssetLibrary, capacity: usize) -> Result<Self> {
        Self::new_with_budget(initial, assets, capacity, DEFAULT_HISTORY_METADATA_BYTES)
    }
    pub fn new_with_budget(
        initial: Arrangement,
        assets: &AssetLibrary,
        capacity: usize,
        max_serialized_bytes: usize,
    ) -> Result<Self> {
        ensure!(
            (1..=64).contains(&capacity),
            "Arrangement history capacity must be 1 to 64"
        );
        initial.validate(assets)?;
        let bytes = state_bytes(&initial)?;
        ensure!(
            bytes + envelope_bytes(capacity, 0, 0) <= max_serialized_bytes,
            "Current arrangement cannot fit the history metadata budget"
        );
        Ok(Self {
            current: initial,
            undo: vec![],
            redo: vec![],
            capacity,
            current_bytes: bytes,
            state_bytes: bytes,
            max_serialized_bytes,
            sequence: 0,
        })
    }
    pub fn current(&self) -> &Arrangement {
        &self.current
    }
    /// Monotonic within this history, including undo/redo; draft previews do not change it.
    pub fn revision(&self) -> u64 {
        self.sequence
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn apply(&mut self, edit: &Edit, assets: &AssetLibrary) -> Result<()> {
        self.apply_batch(std::slice::from_ref(edit), assets)
    }
    pub fn apply_batch(&mut self, edits: &[Edit], assets: &AssetLibrary) -> Result<()> {
        self.apply_batch_checked(edits, assets, |_| Ok(()))
    }
    /// Adapters may bound their complete transport envelope before history changes.
    pub fn apply_batch_checked(
        &mut self,
        edits: &[Edit],
        assets: &AssetLibrary,
        admit: impl FnOnce(&Arrangement) -> Result<()>,
    ) -> Result<()> {
        let next = apply_edits(&self.current, edits, assets)?;
        admit(&next)?;
        if next != self.current {
            let bytes = state_bytes(&next)?;
            ensure!(
                bytes + envelope_bytes(self.capacity, 0, 0) <= self.max_serialized_bytes,
                "Current arrangement cannot fit the history metadata budget"
            );
            let sequence = self.next_sequence()?;
            let redo_bytes = self.redo.iter().map(|state| state.bytes).sum::<usize>();
            let total = self
                .state_bytes
                .checked_sub(redo_bytes)
                .and_then(|value| value.checked_add(bytes))
                .context("Arrangement history size overflow")?;
            self.undo.push(RetainedState {
                document: std::mem::replace(&mut self.current, next),
                bytes: std::mem::replace(&mut self.current_bytes, bytes),
                sequence,
            });
            self.redo.clear();
            self.state_bytes = total;
            self.sequence = sequence;
            self.enforce_limits();
        }
        Ok(())
    }
    pub fn undo(&mut self) -> Result<&Arrangement> {
        let sequence = self.next_sequence()?;
        let previous = self.undo.pop().context("No arrangement undo step")?;
        self.redo.push(RetainedState {
            document: std::mem::replace(&mut self.current, previous.document),
            bytes: std::mem::replace(&mut self.current_bytes, previous.bytes),
            sequence,
        });
        self.sequence = sequence;
        self.enforce_limits();
        Ok(&self.current)
    }
    pub fn redo(&mut self) -> Result<&Arrangement> {
        let sequence = self.next_sequence()?;
        let next = self.redo.pop().context("No arrangement redo step")?;
        self.undo.push(RetainedState {
            document: std::mem::replace(&mut self.current, next.document),
            bytes: std::mem::replace(&mut self.current_bytes, next.bytes),
            sequence,
        });
        self.sequence = sequence;
        self.enforce_limits();
        Ok(&self.current)
    }
    pub fn referenced_sources(&self) -> std::collections::BTreeSet<String> {
        self.undo
            .iter()
            .chain(&self.redo)
            .map(|state| &state.document)
            .chain(std::iter::once(&self.current))
            .flat_map(|state| &state.lanes)
            .flat_map(|lane| &lane.clips)
            .map(|clip| clip.source_id.clone())
            .collect()
    }
}

#[cfg(test)]
#[path = "history_budget_tests.rs"]
mod budget_tests;
