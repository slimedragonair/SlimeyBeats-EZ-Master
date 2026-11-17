//! Producer-owned coalescing and bounded, callback-owned control/retirement bundles.
use super::arrangement_mailbox::{FastUpdate, State};
use crate::studio::arrangement::ScopedEq;

pub(super) const CONTROL_BUNDLE_CAPACITY: usize = 16;
pub(super) struct ControlSlot {
    pub key: String,
    pub update: FastUpdate,
    pub retired_eq: Option<ScopedEq>,
}
pub(super) struct ControlBundle {
    pub epoch: u64,
    pub cursor: usize,
    pub slots: Vec<ControlSlot>,
}
impl State {
    /// Only producers/worker touch map nodes. An offered bundle has not yet been
    /// taken by the callback, so newer controls can safely coalesce into it here.
    pub fn reclaim_controls(&mut self) {
        if let Some(bundle) = self.control_ready.take() {
            for slot in bundle.slots {
                self.fast.insert(slot.key, slot.update);
            }
        }
    }
    pub fn prepare_controls(&mut self, epoch: u64) {
        if self.control_ready.is_some() || self.fast.is_empty() {
            return;
        }
        let mut slots = Vec::with_capacity(self.fast.len().min(CONTROL_BUNDLE_CAPACITY));
        for _ in 0..CONTROL_BUNDLE_CAPACITY {
            let Some((key, update)) = self.fast.pop_first() else {
                break;
            };
            slots.push(ControlSlot {
                key,
                update,
                retired_eq: None,
            });
        }
        self.control_ready = Some(Box::new(ControlBundle {
            epoch,
            cursor: 0,
            slots,
        }));
    }
}
