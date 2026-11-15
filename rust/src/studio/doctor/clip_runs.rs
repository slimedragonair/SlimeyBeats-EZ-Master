use super::thresholds::*;
use crate::audio::Audio;
use anyhow::{Result, ensure};
use std::ops::Range;

#[derive(Clone, Copy)]
pub(super) struct ClipRun {
    pub start: usize,
    pub end: usize,
    pub minimum: f64,
    pub peak: f64,
    pub sign: f64,
}

impl ClipRun {
    pub fn is_plateau(self) -> bool {
        self.end - self.start >= MIN_CLIP_FRAMES && self.peak - self.minimum <= PLATEAU_TOLERANCE
    }
    pub fn is_issue(self) -> bool {
        self.is_plateau() || self.peak > 1.
    }
    pub fn repairable(self, audio: &Audio, channel: usize) -> bool {
        let max_run = ((audio.sample_rate as f64 * MAX_REPAIR_SECONDS).round() as usize)
            .min(MAX_REPAIR_FRAMES);
        if !self.is_plateau()
            || self.peak > 1.001
            || self.end - self.start > max_run
            || self.start < 2
            || self.end + 1 >= audio.frames()
        {
            return false;
        }
        // Both slopes and adjacent samples must belong to the same undamaged
        // lobe. Opposite signs, DC plateaus and boundary-spanning runs abstain.
        let at = |frame| audio.samples[frame * audio.channels + channel] as f64 * self.sign;
        let (before, left, right, after) = (
            at(self.start - 2),
            at(self.start - 1),
            at(self.end),
            at(self.end + 1),
        );
        before >= 0.
            && after >= 0.
            && left >= 0.
            && right >= 0.
            && before < FULL_SCALE
            && after < FULL_SCALE
            && left < FULL_SCALE
            && right < FULL_SCALE
            && left > before
            && right > after
    }
}

pub(super) fn visit(
    audio: &Audio,
    range: Range<usize>,
    channel: usize,
    cancel: &dyn Fn() -> bool,
    mut emit: impl FnMut(ClipRun),
) -> Result<()> {
    let mut run: Option<ClipRun> = None;
    for frame in range.clone() {
        if (frame - range.start).is_multiple_of(CANCEL_FRAMES) {
            ensure!(!cancel(), "Cancelled");
        }
        let sample = audio.samples[frame * audio.channels + channel] as f64;
        let magnitude = sample.abs();
        if let Some(current) = run.as_ref()
            && (magnitude < FULL_SCALE || sample.signum() != current.sign)
        {
            emit(run.take().unwrap());
        }
        if magnitude >= FULL_SCALE {
            let current = run.get_or_insert(ClipRun {
                start: frame,
                end: frame,
                minimum: magnitude,
                peak: magnitude,
                sign: sample.signum(),
            });
            current.end = frame + 1;
            current.minimum = current.minimum.min(magnitude);
            current.peak = current.peak.max(magnitude);
        }
    }
    if let Some(current) = run {
        emit(current);
    }
    ensure!(!cancel(), "Cancelled");
    Ok(())
}
