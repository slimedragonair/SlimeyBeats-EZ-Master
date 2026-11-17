//! ABSource adapter with coalesced background geometry, bounded catch-up and live controls.
use super::{
    arrangement_controls::{ControlBundle, ControlSlot},
    arrangement_mailbox::{ArrangementMailbox, FastUpdate, Prepared, Retired, Shared, State},
    arrangement_worker,
    player::Controls,
};
use crate::{dsp::eq_chain::Frame, studio::arrangement::ArrangementPlayer};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
};

const MAX_CATCHUP_FRAMES_PER_TICK: usize = 4;
pub(super) struct LiveArrangementPath {
    pub processor: ArrangementPlayer,
    pub updates: ArrangementMailbox,
    pending: Option<Prepared>,
    deferred: Option<Retired>,
    controls: Option<Box<ControlBundle>>,
    seeking: bool,
    ticks: usize,
    fade_in: usize,
}
impl LiveArrangementPath {
    pub fn new(processor: ArrangementPlayer, controls: Arc<Controls>) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::sync_channel(8);
        let shared = Arc::new(Shared {
            writer: Mutex::new(()),
            state: Mutex::new(State {
                document: processor.arrangement().clone(),
                assets: processor.assets().clone(),
                options: processor.render_options().clone(),
                request: false,
                ready: None,
                fast: BTreeMap::new(),
                control_ready: None,
                error: None,
            }),
            wake: Condvar::new(),
            epoch: AtomicU64::new(0),
            seek: AtomicU64::new(u64::MAX),
            closed: AtomicBool::new(false),
            preparing: AtomicBool::new(false),
            duration: AtomicU64::new(processor.duration_frames()),
            controls,
            retired: tx,
            rate: processor.sample_rate(),
            channels: processor.channels(),
            #[cfg(test)]
            hold_preparation: AtomicBool::new(false),
            #[cfg(test)]
            preparation_entered: AtomicBool::new(false),
            #[cfg(test)]
            hold_retirement: AtomicBool::new(false),
            #[cfg(test)]
            retirement_held: AtomicBool::new(false),
        });
        arrangement_worker::start(shared.clone(), processor.validation_cache(), rx)?;
        Ok(Self {
            processor,
            updates: ArrangementMailbox(shared),
            pending: None,
            deferred: None,
            controls: None,
            seeking: false,
            ticks: 0,
            fade_in: 0,
        })
    }
    pub fn duration_frames(&self) -> u64 {
        self.updates.0.duration.load(Ordering::Acquire)
    }
    pub fn seeking(&self) -> bool {
        self.seeking
    }
    pub fn request_seek(&mut self, frame: u64) {
        self.updates.request_seek(frame.min(self.duration_frames()));
        self.seeking = true;
    }
    /// Called between complete interleaved frames, including while paused.
    pub fn maintenance(&mut self, index: u64) -> Option<u64> {
        self.ticks = self.ticks.wrapping_add(1);
        if let Some(item) = self.deferred.take() {
            self.deferred = self.updates.retire(item);
        }
        if self.deferred.is_some() {
            return None;
        }
        match self.processor.take_retired() {
            Ok(Some(player)) => self.deferred = self.updates.retire(Retired::Player(player)),
            Err(_) => self.fail(),
            _ => {}
        }
        if self.deferred.is_some() {
            return None;
        }
        let epoch = self.updates.0.epoch.load(Ordering::Acquire);
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.epoch != epoch)
        {
            let old = self.pending.take().unwrap();
            self.deferred = self.updates.retire(Retired::Player(old.player));
        }
        if self.deferred.is_some() {
            return None;
        }
        if self
            .controls
            .as_ref()
            .is_some_and(|bundle| bundle.epoch != epoch)
        {
            let bundle = self.controls.take().unwrap();
            self.deferred = self.updates.retire(Retired::Controls(bundle));
        }
        if self.deferred.is_some() {
            return None;
        }
        if self.ticks.is_multiple_of(64) {
            let ready = self
                .updates
                .0
                .state
                .try_lock()
                .ok()
                .and_then(|mut state| state.ready.take());
            if let Some(ready) = ready {
                if ready.epoch == epoch && self.pending.is_none() {
                    self.pending = Some(ready);
                } else {
                    self.deferred = self.updates.retire(Retired::Player(ready.player));
                }
            }
            if self.deferred.is_none() {
                if self.controls.is_none() {
                    self.controls = self
                        .updates
                        .0
                        .state
                        .try_lock()
                        .ok()
                        .and_then(|mut state| state.control_ready.take());
                    self.updates.0.wake.notify_one();
                }
                if let Some(bundle) = &mut self.controls {
                    if bundle.epoch == epoch {
                        let result =
                            Self::apply_fast(&mut self.processor, &mut bundle.slots[bundle.cursor]);
                        bundle.cursor += 1;
                        if result.is_err() {
                            self.fail();
                        }
                    }
                    if self.controls.as_ref().is_some_and(|bundle| {
                        bundle.epoch != epoch || bundle.cursor == bundle.slots.len()
                    }) {
                        let bundle = self.controls.take().unwrap();
                        self.deferred = self.updates.retire(Retired::Controls(bundle));
                    }
                }
            }
        }
        if self.deferred.is_some() {
            return None;
        }
        let pending = self.pending.as_mut()?;
        if pending.seek || self.seeking {
            let pending = self.pending.take().unwrap();
            if pending.epoch != self.updates.0.epoch.load(Ordering::Acquire) {
                self.deferred = self.updates.retire(Retired::Player(pending.player));
                return None;
            }
            let position = pending.player.position();
            match self.processor.apply_prepared_seek_boxed(pending.player) {
                Ok(old) => self.deferred = self.updates.retire(Retired::Player(old)),
                Err(_) => {
                    self.fail();
                    return None;
                }
            }
            self.seeking = false;
            self.fade_in = (self.processor.sample_rate() as f64 * 0.005).round() as usize;
            self.updates.0.preparing.store(false, Ordering::Release);
            return Some(position);
        }
        let target = index
            .max(self.processor.position())
            .min(pending.player.duration_frames());
        for _ in 0..MAX_CATCHUP_FRAMES_PER_TICK {
            if pending.player.position() >= target {
                break;
            }
            if pending.player.next_frame().is_err() {
                self.fail();
                return None;
            }
        }
        if pending.player.position() != target {
            return None;
        }
        let pending = self.pending.take().unwrap();
        if pending.epoch != self.updates.0.epoch.load(Ordering::Acquire) {
            self.deferred = self.updates.retire(Retired::Player(pending.player));
            return None;
        }
        let shortened = target < index || self.processor.position() != target;
        let result = if shortened {
            self.processor
                .apply_prepared_seek_boxed(pending.player)
                .map(Some)
        } else {
            self.processor.replace_prepared_boxed(pending.player)
        };
        match result {
            Ok(Some(old)) => self.deferred = self.updates.retire(Retired::Player(old)),
            Err(_) => self.fail(),
            _ => {}
        }
        self.updates.0.preparing.store(false, Ordering::Release);
        shortened.then_some(target)
    }
    fn apply_fast(processor: &mut ArrangementPlayer, slot: &mut ControlSlot) -> anyhow::Result<()> {
        // New target IDs are already merged into the pending document. They
        // become audible on geometry adoption, not on the previous revision.
        match &mut slot.update {
            FastUpdate::Lane {
                id,
                gain,
                pan,
                mute,
                solo,
            } => {
                if !processor
                    .arrangement()
                    .lanes
                    .iter()
                    .any(|lane| lane.id == id.as_str())
                {
                    return Ok(());
                }
                processor.set_lane_controls(id, *gain, *pan, *mute, *solo)
            }
            FastUpdate::Clip { id, gain } => {
                if !processor
                    .arrangement()
                    .lanes
                    .iter()
                    .flat_map(|lane| &lane.clips)
                    .any(|clip| clip.id == id.as_str())
                {
                    return Ok(());
                }
                processor.set_clip_gain(id, *gain)
            }
            FastUpdate::Output(gain) => processor.set_output_gain_db(*gain),
            FastUpdate::Eq(update) => {
                if !processor
                    .arrangement()
                    .eq
                    .iter()
                    .any(|effect| update.matches_target(effect))
                {
                    return Ok(());
                }
                slot.retired_eq = Some(processor.apply_prepared_eq_in_place(update)?);
                Ok(())
            }
        }
    }
    pub fn next(&mut self, index: u64) -> Frame {
        if self.seeking {
            return [0.; 2];
        }
        if self.processor.position() != index {
            if index >= self.processor.duration_frames() {
                return [0.; 2];
            }
            self.fail();
            return [0.; 2];
        }
        match self.processor.next_frame() {
            Ok(Some(frame)) => {
                let mut frame = frame.map(|value| value as f64);
                if self.fade_in > 0 {
                    let length = (self.processor.sample_rate() as f64 * 0.005)
                        .round()
                        .max(1.);
                    let weight = 1. - self.fade_in as f64 / length;
                    for value in &mut frame {
                        *value *= weight;
                    }
                    self.fade_in -= 1;
                }
                frame
            }
            Ok(None) => [0.; 2],
            Err(_) => {
                self.fail();
                [0.; 2]
            }
        }
    }
    fn fail(&self) {
        self.updates
            .0
            .controls
            .failure
            .store(true, Ordering::Release);
        self.updates
            .0
            .controls
            .playing
            .store(false, Ordering::Release);
    }
}
impl Drop for LiveArrangementPath {
    fn drop(&mut self) {
        self.updates.close();
    }
}

#[cfg(test)]
#[path = "live_arrangement_tests.rs"]
mod tests;
