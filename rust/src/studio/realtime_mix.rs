//! Frame-based stem monitoring. Source buffers stay immutable; controls never render a file.
use super::stem_mix::{MixSettings, StemSet, channel_levels};
use crate::dsp::{eq_chain::Frame, stream_peak_guard::StreamPeakGuard};
use anyhow::Result;
use std::sync::Arc;

const RAMP_SECONDS: f64 = 0.005;

pub struct MixTargets {
    levels: Vec<Frame>,
    protect_peaks: bool,
    ceiling: f64,
}
impl MixTargets {
    pub fn new(bank: &StemSet, settings: &MixSettings) -> Result<Self> {
        settings.validate(bank)?;
        let solo = settings.tracks.iter().any(|track| track.solo);
        let levels = bank
            .tracks
            .iter()
            .map(|stem| {
                let control = settings
                    .tracks
                    .iter()
                    .find(|c| c.kind == stem.kind)
                    .unwrap();
                channel_levels(control, settings.output_gain_db, solo)
            })
            .collect();
        Ok(Self {
            levels,
            protect_peaks: settings.protect_peaks,
            ceiling: settings.ceiling_dbtp,
        })
    }
}

pub struct RealtimeMix {
    bank: Arc<StemSet>,
    current: Vec<Frame>,
    steps: Vec<Frame>,
    targets: MixTargets,
    remaining: usize,
    ramp_frames: usize,
    guard: StreamPeakGuard,
}
impl RealtimeMix {
    pub fn new(bank: Arc<StemSet>, settings: &MixSettings) -> Result<Self> {
        let targets = MixTargets::new(&bank, settings)?;
        let rate = bank.tracks[0].audio.sample_rate;
        Ok(Self {
            current: targets.levels.clone(),
            steps: vec![[0.; 2]; bank.tracks.len()],
            ramp_frames: (RAMP_SECONDS * rate as f64).round().max(1.) as usize,
            remaining: 0,
            guard: StreamPeakGuard::new(rate, targets.ceiling),
            bank,
            targets,
        })
    }
    pub fn set_targets(&mut self, targets: MixTargets) {
        if targets.levels.len() != self.current.len() {
            return;
        }
        for ((step, current), target) in self
            .steps
            .iter_mut()
            .zip(&self.current)
            .zip(&targets.levels)
        {
            *step = [
                (target[0] - current[0]) / self.ramp_frames as f64,
                (target[1] - current[1]) / self.ramp_frames as f64,
            ];
        }
        if self.targets.ceiling != targets.ceiling {
            self.guard.set_ceiling(targets.ceiling);
        }
        self.remaining = self.ramp_frames;
        self.targets = targets;
    }
    pub fn reset(&mut self) {
        self.current.copy_from_slice(&self.targets.levels);
        self.remaining = 0;
        self.guard.reset(self.targets.ceiling);
    }
    // Keep a fixed delay even when protection is bypassed, preserving A/B and seek alignment.
    pub fn delay(&self) -> usize {
        self.guard.delay
    }
    pub fn process(&mut self, frame: usize) -> Frame {
        if self.remaining > 0 {
            self.remaining -= 1;
            for ((current, step), target) in self
                .current
                .iter_mut()
                .zip(&self.steps)
                .zip(&self.targets.levels)
            {
                for channel in 0..2 {
                    current[channel] = if self.remaining == 0 {
                        target[channel]
                    } else {
                        current[channel] + step[channel]
                    };
                }
            }
        }
        let channels = self.bank.tracks[0].audio.channels;
        let mut mixed = [0.; 2];
        for (stem, gains) in self.bank.tracks.iter().zip(&self.current) {
            if frame < stem.audio.frames() {
                for channel in 0..channels {
                    mixed[channel] +=
                        stem.audio.samples[frame * channels + channel] as f64 * gains[channel];
                }
            }
        }
        self.guard
            .process(mixed, channels, self.targets.protect_peaks)
    }
}
