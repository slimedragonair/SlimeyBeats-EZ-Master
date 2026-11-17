use super::{
    ArrangementHistory, DEFAULT_HISTORY_METADATA_BYTES,
    retention::{RetainedState, state_bytes},
};
use crate::studio::arrangement::{Arrangement, AssetLibrary};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistorySnapshot {
    pub current: Arrangement,
    pub undo: Vec<Arrangement>,
    pub redo: Vec<Arrangement>,
    pub capacity: usize,
}

impl ArrangementHistory {
    pub fn snapshot(&self) -> HistorySnapshot {
        HistorySnapshot {
            current: self.current.clone(),
            undo: self
                .undo
                .iter()
                .map(|state| state.document.clone())
                .collect(),
            redo: self
                .redo
                .iter()
                .map(|state| state.document.clone())
                .collect(),
            capacity: self.capacity,
        }
    }

    pub fn from_snapshot(snapshot: HistorySnapshot, assets: &AssetLibrary) -> Result<Self> {
        Self::from_snapshot_with_budget(snapshot, assets, DEFAULT_HISTORY_METADATA_BYTES)
    }
    pub fn from_snapshot_with_budget(
        snapshot: HistorySnapshot,
        assets: &AssetLibrary,
        max_serialized_bytes: usize,
    ) -> Result<Self> {
        ensure!(
            (1..=64).contains(&snapshot.capacity)
                && snapshot.undo.len() <= snapshot.capacity
                && snapshot.redo.len() <= snapshot.capacity,
            "Arrangement history exceeds its retained-state budget"
        );
        let mut history = Self::new_with_budget(
            snapshot.current,
            assets,
            snapshot.capacity,
            max_serialized_bytes,
        )?;
        for (undo, document) in snapshot
            .undo
            .into_iter()
            .map(|document| (true, document))
            .chain(snapshot.redo.into_iter().map(|document| (false, document)))
        {
            document.validate(assets)?;
            let bytes = state_bytes(&document)?;
            history.state_bytes = history
                .state_bytes
                .checked_add(bytes)
                .ok_or_else(|| anyhow::anyhow!("Arrangement history size overflow"))?;
            history.sequence = history.next_sequence()?;
            let state = RetainedState {
                document,
                bytes,
                sequence: history.sequence,
            };
            if undo {
                history.undo.push(state);
            } else {
                history.redo.push(state);
            }
        }
        history.enforce_limits();
        Ok(history)
    }
}
