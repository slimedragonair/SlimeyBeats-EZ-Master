use super::{clip_runs, thresholds::*};
use crate::audio::Audio;
use anyhow::Result;
use std::ops::Range;

pub(super) fn apply(
    input: &Audio,
    output: &mut Audio,
    range: Range<usize>,
    strength: f64,
    cancel: &dyn Fn() -> bool,
) -> Result<(Vec<String>, f64)> {
    let (mut eligible, mut skipped, mut peak_trim_db) = (0_usize, 0_usize, 0_f64);
    for channel in 0..input.channels {
        clip_runs::visit(input, range.clone(), channel, cancel, |run| {
            if !run.is_issue() {
                return;
            }
            if !run.repairable(input, channel) {
                skipped += 1;
                return;
            }
            eligible += 1;
            let at = |frame| input.samples[frame * input.channels + channel] as f64 * run.sign;
            let (left, right) = (at(run.start - 1), at(run.end));
            let count = run.end - run.start;
            let span = (count + 1) as f64;
            let m0 = (left - at(run.start - 2)) * span;
            let m1 = (at(run.end + 1) - right) * span;
            let mut curve = [0_f64; MAX_REPAIR_FRAMES];
            let mut bend = 1_f64;
            let mut raw_peak = 0_f64;
            for (index, value) in curve[..count].iter_mut().enumerate() {
                let t = (index + 1) as f64 / span;
                let baseline = left + (right - left) * t;
                let hermite = (2. * t.powi(3) - 3. * t.powi(2) + 1.) * left
                    + (t.powi(3) - 2. * t.powi(2) + t) * m0
                    + (-2. * t.powi(3) + 3. * t.powi(2)) * right
                    + (t.powi(3) - t.powi(2)) * m1;
                // Preserve the measured shoulders and compress only inferred
                // curvature if it would exceed full scale. This avoids gain
                // changes to the rest of the lobe, selection, or channel.
                *value = (hermite - baseline).max(0.);
                if *value > 0. {
                    bend = bend.min((1. - baseline).max(0.) / *value);
                }
                raw_peak = raw_peak.max(hermite);
            }
            if raw_peak > 1. {
                peak_trim_db = peak_trim_db.max(20. * raw_peak.log10());
            }
            for (index, &curvature) in curve[..count].iter().enumerate() {
                let t = (index + 1) as f64 / span;
                let baseline = left + (right - left) * t;
                let target = (baseline + bend * curvature).clamp(0., 1.) * run.sign;
                let position = (run.start + index) * input.channels + channel;
                let dry = input.samples[position];
                output.samples[position] = super::repair::protected(
                    dry,
                    dry as f64 + strength * (target - dry as f64),
                    &mut peak_trim_db,
                );
            }
        })?;
    }
    Ok((vec![format!("Conservative declip: {eligible} eligible short full-scale plateau runs, {skipped} ineligible candidates skipped. Isolated overshoots, long/DC plateaus, missing slopes and partial selections are not reconstructed."),
        "Interpolation is a bounded estimate, not recovery of the original waveform. Only eligible plateau samples can change; inferred curvature is reduced locally when needed to stay within full scale.".into()], peak_trim_db))
}
