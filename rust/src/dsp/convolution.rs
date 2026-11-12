use crate::audio::Audio;
use rayon::prelude::*;
use rustfft::{FftPlanner, num_complex::Complex32};
use std::f64::consts::PI;

pub fn lowpass_kernel(freq: f64, sr: u32) -> Vec<f32> {
    let freq = freq.clamp(5., sr as f64 * 0.49);
    let count = ((6. * sr as f64 / freq) as usize).clamp(63, 16383) | 1;
    let half = (count / 2) as f64;
    let fc = freq / sr as f64;
    let mut h: Vec<f64> = (0..count)
        .map(|i| {
            let t = i as f64 - half;
            let sinc = if t.abs() < 1e-10 {
                2. * fc
            } else {
                (2. * PI * fc * t).sin() / (PI * t)
            };
            let w = 2. * PI * i as f64 / (count - 1) as f64;
            let window = if count < 4000 {
                0.35875 - 0.48829 * w.cos() + 0.14128 * (2. * w).cos() - 0.01168 * (3. * w).cos()
            } else {
                0.54 - 0.46 * w.cos()
            };
            sinc * window
        })
        .collect();
    let sum = h.iter().sum::<f64>();
    h.iter_mut().for_each(|v| *v /= sum);
    h.into_iter().map(|v| v as f32).collect()
}

/// Overlap-save convolution, trimmed by half the symmetric kernel to retain frame alignment.
pub fn centered(audio: &Audio, kernel: &[f32]) -> Audio {
    let nfft = (kernel.len() * 2).next_power_of_two().max(4096);
    let block = nfft - kernel.len() + 1;
    let mut planner = FftPlanner::<f32>::new();
    let forward = planner.plan_fft_forward(nfft);
    let inverse = planner.plan_fft_inverse(nfft);
    let mut impulse = vec![Complex32::default(); nfft];
    for (i, &v) in kernel.iter().enumerate() {
        impulse[i].re = v;
    }
    forward.process(&mut impulse);
    let channels: Vec<Vec<f32>> = (0..audio.channels)
        .into_par_iter()
        .map(|ch| {
            let mut output = vec![0_f32; audio.frames()];
            let mut spectrum = vec![Complex32::default(); nfft];
            let mut scratch = vec![
                Complex32::default();
                forward
                    .get_inplace_scratch_len()
                    .max(inverse.get_inplace_scratch_len())
            ];
            let half = kernel.len() / 2;
            for start in (0..audio.frames()).step_by(block) {
                for (j, sample) in spectrum.iter_mut().enumerate() {
                    let index = (start + j + half) as isize - (kernel.len() - 1) as isize;
                    *sample = Complex32::new(
                        if index >= 0 && (index as usize) < audio.frames() {
                            audio.samples[index as usize * audio.channels + ch]
                        } else {
                            0.
                        },
                        0.,
                    );
                }
                forward.process_with_scratch(&mut spectrum, &mut scratch);
                for (value, h) in spectrum.iter_mut().zip(&impulse) {
                    *value *= h;
                }
                inverse.process_with_scratch(&mut spectrum, &mut scratch);
                let count = block.min(audio.frames() - start);
                for i in 0..count {
                    output[start + i] = spectrum[kernel.len() - 1 + i].re / nfft as f32;
                }
            }
            output
        })
        .collect();
    let mut out = audio.clone();
    for (i, frame) in out.samples.chunks_exact_mut(audio.channels).enumerate() {
        for ch in 0..audio.channels {
            frame[ch] = channels[ch][i];
        }
    }
    out
}

pub fn lowpass(audio: &Audio, freq: f64) -> Audio {
    if freq >= audio.sample_rate as f64 * 0.48 {
        return audio.clone();
    }
    centered(audio, &lowpass_kernel(freq, audio.sample_rate))
}
pub fn band(audio: &Audio, lo: f64, hi: f64) -> Audio {
    let mut upper = lowpass(audio, hi.min(audio.sample_rate as f64 * 0.48));
    if lo > 5. {
        let lower = lowpass(audio, lo);
        for (u, l) in upper.samples.iter_mut().zip(lower.samples) {
            *u -= l;
        }
    }
    upper
}
