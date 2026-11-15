use super::thresholds::*;
use crate::{audio::Audio, dsp::biquad::Biquad};
use anyhow::{Result, ensure};
use std::{f64::consts::PI, ops::Range};

pub(super) fn apply(
    input: &Audio,
    output: &mut Audio,
    range: Range<usize>,
    strength: f64,
    cancel: &dyn Fn() -> bool,
) -> Result<(Vec<String>, f64)> {
    let cut_db = -MAX_CUT_DB * strength;
    let filter = Biquad::design("bell", BELL_HZ, cut_db, BELL_Q, input.sample_rate);
    let fade = (FADE_SECONDS * input.sample_rate as f64).round().max(1.) as usize;
    // Read a bounded preceding history into the filter, without modifying it.
    let history = (0.2 * input.sample_rate as f64).ceil() as usize;
    let warm_start = range.start.saturating_sub(history);
    let mut state = [[0_f64; 2]; 2];
    let mut peak_trim_db = 0_f64;
    for frame in warm_start..range.end {
        if (frame - warm_start).is_multiple_of(CANCEL_FRAMES) {
            ensure!(!cancel(), "Cancelled");
        }
        let wet = if frame < range.start {
            0.
        } else {
            let edge = (frame - range.start).min(range.end - 1 - frame) as f64 / fade as f64;
            0.5 - 0.5 * (PI * edge.clamp(0., 1.)).cos()
        };
        for (channel, z) in state[..input.channels].iter_mut().enumerate() {
            let position = frame * input.channels + channel;
            let dry = input.samples[position];
            let x = dry as f64;
            let y = filter.b[0] * x + z[0];
            z[0] = filter.b[1] * x - filter.a[1] * y + z[1];
            z[1] = filter.b[2] * x - filter.a[2] * y;
            ensure!(
                y.is_finite() && z.iter().all(|v| v.is_finite()),
                "Doctor EQ produced non-finite audio"
            );
            if wet > 0. {
                output.samples[position] =
                    super::repair::protected(dry, x + wet * (y - x), &mut peak_trim_db);
            }
        }
    }
    ensure!(!cancel(), "Cancelled");
    Ok((vec![format!("Explicit low-mid EQ: {cut_db:.3} dB bell at {BELL_HZ} Hz, Q {BELL_Q}, with {FADE_SECONDS:.3} s raised-cosine edges inside the selected interval. No detector finding is required."),
        "Filter history is warmed from unchanged preceding audio. Only selected samples are processed, with local sample-peak protection if needed; no automatic normalization or trimming.".into()], peak_trim_db))
}
