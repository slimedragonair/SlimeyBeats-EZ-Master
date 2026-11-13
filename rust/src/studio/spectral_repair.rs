use super::stft::{self, Transform};
use crate::{audio::Audio, meters::lin};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

const FFT_SIZE: usize = 2048;
const EDGE_FADE_SECONDS: f64 = 0.02;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepairMode {
    #[default]
    Attenuate,
    Heal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairSelection {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub low_hz: f64,
    pub high_hz: f64,
    pub reduction_db: f64,
    pub mode: RepairMode,
}
impl RepairSelection {
    pub fn validate(&self, audio: &Audio) -> Result<()> {
        ensure!(
            [
                self.start_seconds,
                self.end_seconds,
                self.low_hz,
                self.high_hz,
                self.reduction_db
            ]
            .iter()
            .all(|v| v.is_finite()),
            "Repair values must be finite"
        );
        ensure!(
            self.start_seconds >= 0.
                && self.start_seconds < self.end_seconds
                && self.end_seconds <= audio.duration() + 1e-7,
            "Select a nonempty time region within the track"
        );
        ensure!(
            self.low_hz >= 0.
                && self.low_hz < self.high_hz
                && self.high_hz <= audio.sample_rate as f64 / 2.,
            "Frequency selection must be within 0 Hz and Nyquist"
        );
        ensure!(
            (-60. ..=0.).contains(&self.reduction_db),
            "Repair reduction must be -60 to 0 dB"
        );
        Ok(())
    }
}

pub fn render(
    input: &Audio,
    selection: &RepairSelection,
    cancel: &dyn Fn() -> bool,
) -> Result<Audio> {
    selection.validate(input)?;
    ensure!(!cancel(), "Cancelled");
    if selection.reduction_db == 0. {
        return Ok(input.clone());
    }
    let start = (selection.start_seconds * input.sample_rate as f64).floor() as usize;
    let end =
        ((selection.end_seconds * input.sample_rate as f64).ceil() as usize).min(input.frames());
    let fade = ((EDGE_FADE_SECONDS * input.sample_rate as f64).round() as usize)
        .min((end - start) / 2)
        .max(1);
    let mut references = Vec::new();
    if selection.mode == RepairMode::Heal {
        let margin = FFT_SIZE / 2;
        ensure!(
            start >= margin || end + margin < input.frames(),
            "Healing needs unselected surrounding audio; shorten the selection or use attenuation"
        );
        let left = if start >= margin {
            start - margin
        } else {
            (end + margin).min(input.frames() - 1)
        };
        let right = if end + margin < input.frames() {
            end + margin
        } else {
            left
        };
        let mut plan = Transform::new(FFT_SIZE);
        for ch in 0..input.channels {
            let a = plan
                .at(input, ch, left as isize)
                .iter()
                .map(|v| v.norm())
                .collect::<Vec<_>>();
            let b = plan
                .at(input, ch, right as isize)
                .iter()
                .map(|v| v.norm())
                .collect::<Vec<_>>();
            references.push((a, b));
        }
    }
    let gain = lin(selection.reduction_db) as f32;
    let width = (selection.high_hz - selection.low_hz).max(1.);
    let feather = (input.sample_rate as f64 / FFT_SIZE as f64).min(width / 4.);
    let mut result = stft::render(input, FFT_SIZE, cancel, |ch, center, bins| {
        if center + FFT_SIZE as isize / 2 < start as isize
            || center - FFT_SIZE as isize / 2 > end as isize
        {
            return;
        }
        let time =
            ((center as f64 - start as f64) / (end - start).max(1) as f64).clamp(0., 1.) as f32;
        for (i, bin) in bins.iter_mut().enumerate() {
            let frequency = i.min(FFT_SIZE - i) as f64 * input.sample_rate as f64 / FFT_SIZE as f64;
            if frequency < selection.low_hz || frequency > selection.high_hz {
                continue;
            }
            let edge = ((frequency - selection.low_hz) / feather)
                .min((selection.high_hz - frequency) / feather)
                .clamp(0., 1.) as f32;
            let target = if selection.mode == RepairMode::Attenuate {
                gain
            } else {
                let (a, b) = &references[ch];
                let surrounding = a[i] * (1. - time) + b[i] * time;
                let ratio = (surrounding / bin.norm().max(1e-12)).clamp(0., 1.);
                gain + (1. - gain) * ratio
            };
            *bin *= 1. + edge * (target - 1.);
        }
    })?;
    // The transform spans window edges. Keep every sample outside the explicit
    // selection bit-identical and feather only inside its time boundary.
    for frame in 0..input.frames() {
        let wet = if frame < start || frame >= end {
            0.
        } else if end - start <= 2 {
            1.
        } else {
            let edge = ((frame - start) as f32 / fade as f32)
                .min((end - 1 - frame) as f32 / fade as f32)
                .clamp(0., 1.);
            0.5 - 0.5 * (std::f32::consts::PI * edge).cos()
        };
        for ch in 0..input.channels {
            let index = frame * input.channels + ch;
            result.samples[index] = if wet == 0. {
                input.samples[index]
            } else {
                input.samples[index] + wet * (result.samples[index] - input.samples[index])
            };
        }
    }
    Ok(result)
}
