use crate::audio::Audio;
use anyhow::{Result, ensure};
use rustfft::{Fft, FftPlanner, num_complex::Complex32};
use std::{f32::consts::PI, sync::Arc};

pub(super) struct Transform {
    pub size: usize,
    pub window: Vec<f32>,
    forward: Arc<dyn Fft<f32>>,
    inverse: Arc<dyn Fft<f32>>,
    buffer: Vec<Complex32>,
    scratch: Vec<Complex32>,
}
impl Transform {
    pub fn new(size: usize) -> Self {
        let mut planner = FftPlanner::new();
        let forward = planner.plan_fft_forward(size);
        let inverse = planner.plan_fft_inverse(size);
        let scratch = vec![
            Complex32::default();
            forward
                .get_inplace_scratch_len()
                .max(inverse.get_inplace_scratch_len())
        ];
        let window = (0..size)
            .map(|i| (0.5 - 0.5 * (2. * PI * i as f32 / size as f32).cos()).sqrt())
            .collect();
        Self {
            size,
            window,
            forward,
            inverse,
            buffer: vec![Complex32::default(); size],
            scratch,
        }
    }
    pub fn at(&mut self, audio: &Audio, ch: usize, center: isize) -> &[Complex32] {
        for (i, value) in self.buffer.iter_mut().enumerate() {
            let position = center + i as isize - self.size as isize / 2;
            let sample = if position >= 0 && (position as usize) < audio.frames() {
                audio.samples[position as usize * audio.channels + ch]
            } else {
                0.
            };
            *value = Complex32::new(sample * self.window[i], 0.);
        }
        self.forward
            .process_with_scratch(&mut self.buffer, &mut self.scratch);
        &self.buffer
    }
}

/// Centred, padded overlap-add preserves frame alignment and boundary coverage.
pub(super) fn render(
    audio: &Audio,
    size: usize,
    cancel: &dyn Fn() -> bool,
    mut edit: impl FnMut(usize, isize, &mut [Complex32]),
) -> Result<Audio> {
    let mut plan = Transform::new(size);
    let hop = size / 4;
    let mut result = audio.clone();
    for ch in 0..audio.channels {
        let mut output = vec![0_f32; audio.frames()];
        let mut weight = vec![0_f32; audio.frames()];
        for center in (0..audio.frames() + size / 2).step_by(hop) {
            ensure!(!cancel(), "Cancelled");
            plan.at(audio, ch, center as isize);
            edit(ch, center as isize, &mut plan.buffer);
            plan.inverse
                .process_with_scratch(&mut plan.buffer, &mut plan.scratch);
            for i in 0..size {
                let index = center as isize + i as isize - size as isize / 2;
                if index < 0 || index as usize >= audio.frames() {
                    continue;
                }
                let p = index as usize;
                output[p] += plan.buffer[i].re * plan.window[i] / size as f32;
                weight[p] += plan.window[i] * plan.window[i];
            }
        }
        for i in 0..audio.frames() {
            result.samples[i * audio.channels + ch] = output[i] / weight[i].max(1e-10);
        }
    }
    ensure!(
        result.samples.iter().all(|v| v.is_finite()),
        "Spectral processing produced non-finite audio"
    );
    Ok(result)
}
