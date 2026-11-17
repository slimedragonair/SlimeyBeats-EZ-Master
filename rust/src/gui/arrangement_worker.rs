//! Native preparation and disposal run on this worker, never on ABSource's callback.
use super::{
    arrangement_mailbox::{Prepared, Retired, Shared},
    player::Controls,
};
use crate::studio::arrangement::{ArrangementPlayer, AssetValidationCache};
use std::{
    sync::{Arc, atomic::Ordering, mpsc::Receiver},
    time::Duration,
};

pub(super) fn start(
    shared: Arc<Shared>,
    mut cache: AssetValidationCache,
    retired: Receiver<Retired>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("arrangement-audio-prepare".into())
        .spawn(move || {
            while !shared.closed.load(Ordering::Acquire) {
                #[cfg(test)]
                let dispose = !shared.hold_retirement.load(Ordering::Acquire);
                #[cfg(test)]
                shared.retirement_held.store(!dispose, Ordering::Release);
                #[cfg(not(test))]
                let dispose = true;
                if dispose {
                    while let Ok(item) = retired.try_recv() {
                        match item {
                            Retired::Player(player) => drop(player),
                            Retired::Controls(bundle) => drop(bundle),
                        }
                    }
                }
                let job = {
                    let Ok(mut state) = shared.state.lock() else {
                        fail_lock(&shared.controls);
                        break;
                    };
                    state.prepare_controls(shared.epoch.load(Ordering::Acquire));
                    let seek = shared.seek.swap(u64::MAX, Ordering::AcqRel);
                    if !state.request && seek == u64::MAX {
                        drop(shared.wake.wait_timeout(state, Duration::from_millis(10)));
                        continue;
                    }
                    let epoch = shared.epoch.load(Ordering::Acquire);
                    let frame = if seek != u64::MAX {
                        seek
                    } else {
                        shared.controls.position.load(Ordering::Acquire)
                    };
                    state.request = false;
                    state.ready = None;
                    (
                        epoch,
                        seek != u64::MAX,
                        frame.min(state.document.duration_frames()),
                        state.document.clone(),
                        state.assets.clone(),
                        state.options.clone(),
                    )
                };
                let (epoch, seek, frame, document, assets, options) = job;
                let cancelled = || {
                    shared.closed.load(Ordering::Acquire)
                        || shared.epoch.load(Ordering::Acquire) != epoch
                };
                #[cfg(test)]
                {
                    shared.preparation_entered.store(true, Ordering::Release);
                    while shared.hold_preparation.load(Ordering::Acquire) && !cancelled() {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                let result = ArrangementPlayer::prepare_bounded(
                    &document,
                    &assets,
                    &options,
                    &mut cache,
                    frame,
                    (document.sample_rate as f64 * 1.5).ceil() as u64,
                    &cancelled,
                )
                .and_then(|mut player| {
                    player.set_replacement_fade_frames(
                        (document.sample_rate as f64 * 0.005).round().max(1.) as usize,
                    )?;
                    Ok(player)
                });
                if cancelled() {
                    continue;
                }
                let Ok(mut state) = shared.state.lock() else {
                    fail_lock(&shared.controls);
                    break;
                };
                if cancelled() {
                    continue;
                }
                match result {
                    Ok(player) => {
                        state.ready = Some(Prepared {
                            epoch,
                            seek,
                            player: Box::new(player),
                        })
                    }
                    Err(error) => {
                        state.error = Some(error.to_string());
                        shared.preparing.store(false, Ordering::Release);
                    }
                }
            }
        })
        .map(|_| ())
        .map_err(|error| {
            anyhow::anyhow!("Cannot start arrangement audio preparation worker: {error}")
        })
}
fn fail_lock(controls: &Controls) {
    controls.failure.store(true, Ordering::Release);
}
