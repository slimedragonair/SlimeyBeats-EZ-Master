use crate::{
    audio::Audio,
    dsp::{biquad, dynamics::HOP},
};
use rustfft::{FftPlanner, num_complex::Complex32};
use std::f64::consts::PI;

pub fn cleanup(audio: &mut Audio, hpf: f64, flip: bool) {
    for ch in 0..audio.channels {
        let mean = audio
            .samples
            .iter()
            .skip(ch)
            .step_by(audio.channels)
            .map(|&v| v as f64)
            .sum::<f64>()
            / audio.frames() as f64;
        for sample in audio.samples.iter_mut().skip(ch).step_by(audio.channels) {
            *sample -= mean as f32;
        }
    }
    if hpf > 0. {
        biquad::highpass(audio, hpf);
    }
    if flip && audio.channels == 2 {
        for frame in audio.samples.as_chunks_mut::<2>().0 {
            frame[1] = -frame[1];
        }
    }
}

pub fn declip(audio: &mut Audio, original: &Audio, max_run: usize) -> usize {
    let mut fixed = 0;
    for ch in 0..audio.channels {
        let mut i = 1;
        while i + 2 < audio.frames() {
            if original.samples[i * audio.channels + ch].abs() < 0.9999 {
                i += 1;
                continue;
            }
            let start = i;
            while i < audio.frames() && original.samples[i * audio.channels + ch].abs() >= 0.9999 {
                i += 1;
            }
            let count = i - start;
            if count > max_run || start < 2 || i + 1 >= audio.frames() {
                continue;
            }
            let a = audio.samples[(start - 1) * audio.channels + ch] as f64;
            let b = audio.samples[i * audio.channels + ch] as f64;
            let m0 =
                (a - audio.samples[(start - 2) * audio.channels + ch] as f64) * (count + 1) as f64;
            let m1 = (audio.samples[(i + 1) * audio.channels + ch] as f64 - b) * (count + 1) as f64;
            let bound = a.abs().max(b.abs()) * 1.5;
            for j in 0..count {
                let t = (j + 1) as f64 / (count + 1) as f64;
                let y = (2. * t.powi(3) - 3. * t * t + 1.) * a
                    + (t.powi(3) - 2. * t * t + t) * m0
                    + (-2. * t.powi(3) + 3. * t * t) * b
                    + (t.powi(3) - t * t) * m1;
                audio.samples[(start + j) * audio.channels + ch] = y.clamp(-bound, bound) as f32;
            }
            fixed += 1;
        }
    }
    fixed
}

/// Conservative high-frequency Wiener reduction; quiet frames estimate noise, never the intro alone.
pub fn denoise(audio: &mut Audio, amount: f64, protect: &[f32]) {
    let size = 2048;
    let hop = size / 2;
    let window: Vec<_> = (0..size)
        .map(|i| (0.5 - 0.5 * (2. * PI * i as f64 / size as f64).cos()).sqrt() as f32)
        .collect();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(size);
    let inverse = planner.plan_fft_inverse(size);
    let mut buffer = vec![Complex32::default(); size];
    let mut scratch = vec![
        Complex32::default();
        fft.get_inplace_scratch_len()
            .max(inverse.get_inplace_scratch_len())
    ];
    for ch in 0..audio.channels {
        let mut noise = vec![0_f64; size];
        let positions: Vec<_> = (0..audio.frames()).step_by(hop).collect();
        let energies: Vec<_> = positions
            .iter()
            .map(|&s| {
                (s..(s + size).min(audio.frames()))
                    .map(|i| (audio.samples[i * audio.channels + ch] as f64).powi(2))
                    .sum::<f64>()
                    / size as f64
            })
            .collect();
        let threshold = crate::meters::percentile(&energies, 0.1);
        let mut n = 0;
        for (&start, &energy) in positions.iter().zip(&energies) {
            if energy > threshold {
                continue;
            }
            for i in 0..size {
                buffer[i] = Complex32::new(
                    audio
                        .samples
                        .get((start + i) * audio.channels + ch)
                        .copied()
                        .unwrap_or(0.)
                        * window[i],
                    0.,
                );
            }
            fft.process_with_scratch(&mut buffer, &mut scratch);
            for (p, v) in noise.iter_mut().zip(&buffer) {
                *p += v.norm_sqr() as f64;
            }
            n += 1;
        }
        noise.iter_mut().for_each(|v| *v /= n.max(1) as f64);
        let mut output = vec![0_f32; audio.frames() + size];
        let mut weights = vec![0_f32; output.len()];
        // Centre the first window before frame zero so the leading audio has full overlap coverage.
        for start in (0..audio.frames() + hop).step_by(hop) {
            for i in 0..size {
                let position = (start + i) as isize - hop as isize;
                buffer[i] = Complex32::new(
                    if position >= 0 && (position as usize) < audio.frames() {
                        audio.samples[position as usize * audio.channels + ch] * window[i]
                    } else {
                        0.
                    },
                    0.,
                );
            }
            fft.process_with_scratch(&mut buffer, &mut scratch);
            for (i, v) in buffer.iter_mut().enumerate() {
                let frequency = i.min(size - i) as f64 * audio.sample_rate as f64 / size as f64;
                if frequency < 4000. {
                    continue;
                }
                let power = v.norm_sqr() as f64;
                let gain = (1. - amount * noise[i] / power.max(1e-20)).clamp(0.794, 1.); // no more than 2 dB per bin
                *v *= gain as f32;
            }
            inverse.process_with_scratch(&mut buffer, &mut scratch);
            for i in 0..size {
                let p = (start + i) as isize - hop as isize;
                if p >= 0 && (p as usize) < output.len() {
                    output[p as usize] += buffer[i].re * window[i] / size as f32;
                    weights[p as usize] += window[i] * window[i];
                }
            }
        }
        for i in 0..audio.frames() {
            let p = protect.get(i / HOP).copied().unwrap_or(0.);
            let dry = audio.samples[i * audio.channels + ch];
            audio.samples[i * audio.channels + ch] =
                dry * p + (1. - p) * output[i] / weights[i].max(1e-8);
        }
    }
}
