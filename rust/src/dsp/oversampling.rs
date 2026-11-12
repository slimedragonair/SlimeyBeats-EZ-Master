use crate::audio::Audio;
use rayon::prelude::*;
use std::f64::consts::PI;

pub(super) fn bessel(x: f64) -> f64 {
    let (mut sum, mut term) = (1., 1.);
    for k in 1..35 {
        term *= (x / (2. * k as f64)).powi(2);
        sum += term;
        if term < sum * 1e-16 {
            break;
        }
    }
    sum
}
fn kernel(factor: usize) -> Vec<f64> {
    let half = 10 * factor;
    let denom = bessel(5.);
    let mut taps: Vec<_> = (0..=half * 2)
        .map(|i| {
            let t = i as f64 - half as f64;
            let x = t / factor as f64;
            let sinc = if x.abs() < 1e-12 {
                1.
            } else {
                (PI * x).sin() / (PI * x)
            };
            sinc * bessel(5. * (1. - (t / half as f64).powi(2)).max(0.).sqrt()) / denom
        })
        .collect();
    let scale = factor as f64 / taps.iter().sum::<f64>();
    taps.iter_mut().for_each(|t| *t *= scale);
    taps
}

/// Compatibility entry point; the limiter and final meter share the stronger detector.
pub fn peak_envelope(audio: &Audio) -> Vec<f32> {
    super::true_peak::envelope(audio)
}

pub fn nonlinear(audio: &Audio, factor: usize, function: impl Fn(f64) -> f64 + Sync) -> Audio {
    let h = kernel(factor);
    let half = 10 * factor;
    let frames = audio.frames();
    let planes: Vec<Vec<f32>> = (0..audio.channels)
        .into_par_iter()
        .map(|ch| {
            let mut up = vec![0_f32; frames * factor];
            for (i, value) in up.iter_mut().enumerate() {
                let frame = i / factor;
                let phase = i % factor;
                let mut y = 0.;
                for offset in -10_isize..=10 {
                    let source = frame as isize + offset;
                    let tap = half as isize + phase as isize - offset * factor as isize;
                    if source >= 0
                        && (source as usize) < frames
                        && (0..h.len() as isize).contains(&tap)
                    {
                        y += audio.samples[source as usize * audio.channels + ch] as f64
                            * h[tap as usize];
                    }
                }
                *value = function(y) as f32;
            }
            let mut down = vec![0_f32; frames];
            for (i, value) in down.iter_mut().enumerate() {
                let mut y = 0.;
                for (k, &tap) in h.iter().enumerate() {
                    let index = (i * factor + k) as isize - half as isize;
                    if index >= 0 && (index as usize) < up.len() {
                        y += up[index as usize] as f64 * tap / factor as f64;
                    }
                }
                *value = y as f32;
            }
            down
        })
        .collect();
    let mut out = audio.clone();
    for (i, frame) in out.samples.chunks_exact_mut(audio.channels).enumerate() {
        for ch in 0..audio.channels {
            frame[ch] = planes[ch][i];
        }
    }
    out
}
