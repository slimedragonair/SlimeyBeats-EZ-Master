use crate::audio::Audio;
#[cfg(not(target_arch = "wasm32"))]
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

/// A shared block transform; the destination determines whether channel planes
/// are retained for native parallelism or written directly for bounded browser memory.
struct CenteredConvolution {
    nfft: usize,
    block: usize,
    kernel_len: usize,
    forward: std::sync::Arc<dyn rustfft::Fft<f32>>,
    inverse: std::sync::Arc<dyn rustfft::Fft<f32>>,
    impulse: Vec<Complex32>,
}
impl CenteredConvolution {
    fn new(kernel: &[f32]) -> Self {
        let nfft = (kernel.len() * 2).next_power_of_two().max(4096);
        let mut planner = FftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(nfft);
        let inverse = planner.plan_fft_inverse(nfft);
        let mut impulse = vec![Complex32::default(); nfft];
        for (i, &value) in kernel.iter().enumerate() {
            impulse[i].re = value;
        }
        forward.process(&mut impulse);
        Self {
            nfft,
            block: nfft - kernel.len() + 1,
            kernel_len: kernel.len(),
            forward,
            inverse,
            impulse,
        }
    }
    fn channel(&self, audio: &Audio, ch: usize, mut emit: impl FnMut(usize, f32)) {
        let mut spectrum = vec![Complex32::default(); self.nfft];
        let mut scratch = vec![
            Complex32::default();
            self.forward
                .get_inplace_scratch_len()
                .max(self.inverse.get_inplace_scratch_len())
        ];
        let half = self.kernel_len / 2;
        for start in (0..audio.frames()).step_by(self.block) {
            for (j, sample) in spectrum.iter_mut().enumerate() {
                let index = (start + j + half) as isize - (self.kernel_len - 1) as isize;
                *sample = Complex32::new(
                    if index >= 0 && (index as usize) < audio.frames() {
                        audio.samples[index as usize * audio.channels + ch]
                    } else {
                        0.
                    },
                    0.,
                );
            }
            self.forward
                .process_with_scratch(&mut spectrum, &mut scratch);
            for (value, h) in spectrum.iter_mut().zip(&self.impulse) {
                *value *= h;
            }
            self.inverse
                .process_with_scratch(&mut spectrum, &mut scratch);
            let count = self.block.min(audio.frames() - start);
            for i in 0..count {
                emit(
                    start + i,
                    spectrum[self.kernel_len - 1 + i].re / self.nfft as f32,
                );
            }
        }
    }
    #[cfg(any(target_arch = "wasm32", test))]
    fn interleaved(&self, audio: &Audio) -> Audio {
        let mut output = Audio {
            samples: vec![0.; audio.samples.len()],
            sample_rate: audio.sample_rate,
            channels: audio.channels,
        };
        for ch in 0..audio.channels {
            self.channel(audio, ch, |frame, value| {
                output.samples[frame * audio.channels + ch] = value
            });
        }
        output
    }
}

/// Overlap-save convolution, trimmed by half the symmetric kernel to retain frame alignment.
pub fn centered(audio: &Audio, kernel: &[f32]) -> Audio {
    let transform = CenteredConvolution::new(kernel);
    #[cfg(target_arch = "wasm32")]
    {
        transform.interleaved(audio)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let channels: Vec<Vec<f32>> = (0..audio.channels)
            .into_par_iter()
            .map(|ch| {
                let mut output = vec![0_f32; audio.frames()];
                transform.channel(audio, ch, |frame, value| output[frame] = value);
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

#[cfg(test)]
mod memory_path_tests {
    use super::*;
    #[test]
    fn browser_interleaved_output_is_bit_identical_to_native_channel_planes() {
        for channels in [1, 2] {
            for frames in [13, 8197] {
                let samples = (0..frames * channels)
                    .map(|i| ((i * 37 % 101) as f32 - 50.) / 100.)
                    .collect();
                let audio = Audio::new(samples, 48000, channels).unwrap();
                let kernel = lowpass_kernel(1700., 48000);
                let browser = CenteredConvolution::new(&kernel).interleaved(&audio);
                assert_eq!(centered(&audio, &kernel).samples, browser.samples);
            }
        }
    }
}
