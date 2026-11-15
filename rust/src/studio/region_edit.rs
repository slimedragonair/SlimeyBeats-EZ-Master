//! Length-preserving edits shared by studio callers. Times round to the nearest
//! frame (half frames round upward); the selected end frame is exclusive.
//! Rendering copies and validates the source with cancellation before editing
//! only the selected frames. No normalization or peak limiting is applied.

use crate::audio::Audio;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::ops::Range;

const MIN_GAIN_DB: f64 = -60.;
const MAX_GAIN_DB: f64 = 12.;
const CANCEL_CHECK_FRAMES: usize = 16_384;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct RegionEdit {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub operation: RegionOperation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[serde(from = "RegionOperationInput")]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub enum RegionOperation {
    Silence,
    FadeIn,
    FadeOut,
    Gain { db: f64 },
    Reverse,
}

// Serde ignores unknown fields on internally tagged unit variants. Decode via
// empty struct variants to enforce the same strict contract for every operation.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
enum RegionOperationInput {
    Silence {},
    FadeIn {},
    FadeOut {},
    Gain { db: f64 },
    Reverse {},
}

impl From<RegionOperationInput> for RegionOperation {
    fn from(input: RegionOperationInput) -> Self {
        match input {
            RegionOperationInput::Silence {} => Self::Silence,
            RegionOperationInput::FadeIn {} => Self::FadeIn,
            RegionOperationInput::FadeOut {} => Self::FadeOut,
            RegionOperationInput::Gain { db } => Self::Gain { db },
            RegionOperationInput::Reverse {} => Self::Reverse,
        }
    }
}

impl RegionOperation {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Silence => "Silence",
            Self::FadeIn => "Fade in",
            Self::FadeOut => "Fade out",
            Self::Gain { .. } => "Gain",
            Self::Reverse => "Reverse",
        }
    }

    pub fn help(&self) -> &'static str {
        match self {
            Self::Silence => "Replace the selected frames with silence without changing duration.",
            Self::FadeIn => {
                "Linear fade from silence to the original level; a one-frame fade is silent."
            }
            Self::FadeOut => {
                "Linear fade from the original level to silence; a one-frame fade is silent."
            }
            Self::Gain { .. } => {
                "Adjust the selection by -60 to +12 dB without peak limiting; audition for clipping."
            }
            Self::Reverse => {
                "Reverse selected frames while preserving left/right channel positions."
            }
        }
    }
}

impl RegionEdit {
    /// Validate the layout, time range and operation. Rendering also checks PCM
    /// finiteness while copying, so long tracks remain cancellable.
    pub fn validate(&self, input: &Audio) -> Result<()> {
        self.frame_range(input).map(|_| ())
    }

    fn frame_range(&self, input: &Audio) -> Result<Range<usize>> {
        ensure!(
            (8000..=384000).contains(&input.sample_rate),
            "Sample rate must be 8000–384000 Hz"
        );
        ensure!(
            (1..=2).contains(&input.channels),
            "Only mono and stereo are supported; downmix explicitly first"
        );
        ensure!(
            !input.samples.is_empty() && input.samples.len().is_multiple_of(input.channels),
            "Empty or incomplete audio frames"
        );
        ensure!(
            self.start_seconds.is_finite() && self.end_seconds.is_finite(),
            "Region times must be finite"
        );
        ensure!(
            self.start_seconds >= 0.
                && self.start_seconds < self.end_seconds
                && self.end_seconds <= input.duration(),
            "Select a nonempty time region within the track"
        );
        if let RegionOperation::Gain { db } = &self.operation {
            ensure!(
                db.is_finite() && (MIN_GAIN_DB..=MAX_GAIN_DB).contains(db),
                "Region gain must be finite and -60 to +12 dB"
            );
        }
        let start = (self.start_seconds * input.sample_rate as f64).round() as usize;
        let end = (self.end_seconds * input.sample_rate as f64).round() as usize;
        ensure!(
            start < end && end <= input.frames(),
            "Time region must select at least one complete frame after rounding"
        );
        Ok(start..end)
    }
}

/// Return an independent revision; unselected samples retain their exact bits.
/// Fades share a linear envelope across channels, including both endpoints.
/// A one-frame fade is silent in either direction. Gain may exceed unity, but
/// any non-finite input or result outside the finite f32 range is rejected.
pub fn render(input: &Audio, edit: &RegionEdit, cancel: &dyn Fn() -> bool) -> Result<Audio> {
    ensure!(!cancel(), "Cancelled");
    let range = edit.frame_range(input)?;
    ensure!(!cancel(), "Cancelled");
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(input.samples.len())
        .context("Cannot allocate region edit revision")?;
    for chunk in input.samples.chunks(CANCEL_CHECK_FRAMES * input.channels) {
        ensure!(!cancel(), "Cancelled");
        ensure!(
            chunk.iter().all(|sample| sample.is_finite()),
            "Audio contains NaN or infinite samples"
        );
        samples.extend_from_slice(chunk);
    }
    ensure!(!cancel(), "Cancelled");
    let selected = &mut samples[range.start * input.channels..range.end * input.channels];
    let frames = range.len();
    if matches!(edit.operation, RegionOperation::Reverse) {
        for frame in 0..frames / 2 {
            if frame % CANCEL_CHECK_FRAMES == 0 {
                ensure!(!cancel(), "Cancelled");
            }
            for channel in 0..input.channels {
                selected.swap(
                    frame * input.channels + channel,
                    (frames - 1 - frame) * input.channels + channel,
                );
            }
        }
    } else {
        let gain = match edit.operation {
            RegionOperation::Gain { db } => 10_f64.powf(db / 20.),
            _ => 1.,
        };
        for (index, frame) in selected.chunks_exact_mut(input.channels).enumerate() {
            if index % CANCEL_CHECK_FRAMES == 0 {
                ensure!(!cancel(), "Cancelled");
            }
            let level = match edit.operation {
                RegionOperation::Silence => 0.,
                RegionOperation::FadeIn | RegionOperation::FadeOut if frames == 1 => 0.,
                RegionOperation::FadeIn => index as f64 / (frames - 1) as f64,
                RegionOperation::FadeOut => (frames - 1 - index) as f64 / (frames - 1) as f64,
                _ => gain,
            };
            if level == 0. {
                frame.fill(0.);
                continue;
            }
            for sample in frame {
                let value = *sample as f64 * level;
                ensure!(
                    value.is_finite() && value.abs() <= f32::MAX as f64,
                    "Region edit overflowed; reduce the selection gain"
                );
                *sample = value as f32;
            }
        }
    }
    ensure!(!cancel(), "Cancelled");
    Ok(Audio {
        samples,
        sample_rate: input.sample_rate,
        channels: input.channels,
    })
}

#[cfg(test)]
#[path = "region_edit_tests.rs"]
mod tests;
