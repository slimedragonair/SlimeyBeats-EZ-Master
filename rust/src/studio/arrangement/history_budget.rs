use super::ArrangementHistory;
use crate::studio::arrangement::{Arrangement, MAX_METADATA_BYTES, budget::serialized_bytes};
use anyhow::{Context, Result};

pub const DEFAULT_HISTORY_METADATA_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct RetainedState {
    pub document: Arrangement,
    pub bytes: usize,
    pub sequence: u64,
}

pub(super) fn state_bytes(document: &Arrangement) -> Result<usize> {
    serialized_bytes(document, MAX_METADATA_BYTES)
}

pub(super) fn envelope_bytes(capacity: usize, undo: usize, redo: usize) -> usize {
    // Exact compact HistorySnapshot JSON overhead, excluding document payloads.
    b"{\"current\":,\"undo\":[],\"redo\":[],\"capacity\":}".len()
        + capacity.ilog10() as usize
        + 1
        + undo.saturating_sub(1)
        + redo.saturating_sub(1)
}

impl ArrangementHistory {
    /// O(1): cached payload counts plus array/envelope punctuation.
    pub fn metadata_bytes(&self) -> usize {
        self.state_bytes + envelope_bytes(self.capacity, self.undo.len(), self.redo.len())
    }
    pub fn metadata_budget_bytes(&self) -> usize {
        self.max_serialized_bytes
    }
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub(super) fn next_sequence(&self) -> Result<u64> {
        self.sequence
            .checked_add(1)
            .context("Arrangement history sequence exhausted")
    }
    pub(super) fn enforce_limits(&mut self) {
        while self.undo.len() > self.capacity {
            self.evict_undo();
        }
        while self.redo.len() > self.capacity {
            self.evict_redo();
        }
        while self.metadata_bytes() > self.max_serialized_bytes {
            let oldest_is_undo = match (self.undo.first(), self.redo.first()) {
                (Some(undo), Some(redo)) => undo.sequence <= redo.sequence,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => {
                    unreachable!("Current state was admitted against its history budget")
                }
            };
            if oldest_is_undo {
                self.evict_undo();
            } else {
                self.evict_redo();
            }
        }
    }
    fn evict_undo(&mut self) {
        self.state_bytes -= self.undo.remove(0).bytes;
    }
    fn evict_redo(&mut self) {
        self.state_bytes -= self.redo.remove(0).bytes;
    }
}
