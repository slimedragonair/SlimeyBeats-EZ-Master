//! Bounded latest-control mailbox for the native audio callback.
use crate::studio::realtime_mix::{MixTargets, RealtimeMix};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(super) struct MixMailbox(Arc<Mutex<Option<MixTargets>>>);
impl MixMailbox {
    pub fn send(&self, targets: MixTargets) -> bool {
        match self.0.lock() {
            Ok(mut pending) => {
                *pending = Some(targets);
                true
            }
            Err(_) => false,
        }
    }
    pub fn take(&self) -> Option<MixTargets> {
        // The audio thread never waits for the UI; a contended update is read next block.
        self.0
            .try_lock()
            .ok()
            .and_then(|mut pending| pending.take())
    }
}
pub(super) struct LiveStemPath {
    pub processor: RealtimeMix,
    pub updates: MixMailbox,
    pub active: bool,
}
