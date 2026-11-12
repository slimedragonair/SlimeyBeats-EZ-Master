//! Offline reconstruction for final measurement and limiter detection; never runs in an audio callback.
use super::oversampling::bessel;
use crate::audio::Audio;
use rayon::prelude::*;
use std::{f64::consts::PI, sync::OnceLock};

const HALF: usize = 32;
const TAPS: usize = HALF * 2;
const PHASES: [f64; 7] = [0.5, 0.25, 0.75, 0.125, 0.375, 0.625, 0.875];
const CHUNK: usize = 1024;
type Kernels = [[f64; TAPS]; 7];

fn kernels() -> &'static Kernels {
    static CACHE: OnceLock<Kernels> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut result = [[0.; TAPS]; 7];
        let denominator = bessel(9.);
        for (taps, fraction) in result.iter_mut().zip(PHASES) {
            for (i, tap) in taps.iter_mut().enumerate() {
                let distance = i as f64 - (HALF - 1) as f64 - fraction;
                let window = bessel(9. * (1. - (distance / HALF as f64).powi(2)).max(0.).sqrt())
                    / denominator;
                *tap = (PI * distance).sin() / (PI * distance) * window;
            }
            let sum: f64 = taps.iter().sum();
            for tap in taps {
                *tap /= sum;
            }
        }
        result
    })
}

fn interpolated_peak(audio: &Audio, frame: isize) -> f64 {
    let mut maximum = 0_f64;
    for channel in 0..audio.channels {
        let mut samples = [0_f64; TAPS];
        let first = frame - (HALF - 1) as isize;
        for (i, sample) in samples.iter_mut().enumerate() {
            let index = first + i as isize;
            if index >= 0 && (index as usize) < audio.frames() {
                *sample = audio.samples[index as usize * audio.channels + channel] as f64;
            }
        }
        if frame >= 0 && (frame as usize) < audio.frames() {
            maximum =
                maximum.max(audio.samples[frame as usize * audio.channels + channel].abs() as f64);
        }
        for taps in kernels() {
            // Independent accumulators allow vectorization without reassociating a serial reduction.
            let mut sums = [0.; 4];
            for i in (0..TAPS).step_by(4) {
                for lane in 0..4 {
                    sums[lane] += samples[i + lane] * taps[i + lane];
                }
            }
            maximum = maximum.max(sums.iter().sum::<f64>().abs());
        }
    }
    maximum
}

pub fn peak(audio: &Audio, known_peak: f64) -> f64 {
    let length = audio.frames() + HALF * 2;
    let bound = kernels()
        .iter()
        .map(|taps| taps.iter().map(|tap| tap.abs()).sum::<f64>())
        .fold(1., f64::max);
    (0..length.div_ceil(CHUNK))
        .into_par_iter()
        .map(|chunk| {
            let start = chunk * CHUNK;
            let stop = (start + CHUNK).min(length);
            let first_source = start.saturating_sub(HALF * 2);
            let last_source = stop.min(audio.frames());
            let local = if first_source < last_source {
                audio.samples[first_source * audio.channels..last_source * audio.channels]
                    .iter()
                    .map(|value| value.abs() as f64)
                    .fold(0., f64::max)
            } else {
                0.
            };
            if local * bound <= known_peak {
                return known_peak;
            }
            (start..stop)
                .map(|index| interpolated_peak(audio, index as isize - HALF as isize))
                .fold(known_peak, f64::max)
        })
        .reduce(|| known_peak, f64::max)
}

pub fn envelope(audio: &Audio) -> Vec<f32> {
    let mut values: Vec<_> = (0..audio.frames())
        .into_par_iter()
        .map(|frame| interpolated_peak(audio, frame as isize) as f32)
        .collect();
    // Include the reconstruction tail on both ends, so short hot endings are not missed.
    if let (Some(first), Some(last)) = (values.first().copied(), values.last().copied()) {
        values[0] = (-((HALF) as isize)..0)
            .map(|frame| interpolated_peak(audio, frame) as f32)
            .fold(first, f32::max);
        let end = audio.frames();
        values[end - 1] = (end..end + HALF)
            .map(|frame| interpolated_peak(audio, frame as isize) as f32)
            .fold(last, f32::max);
    }
    values
}

#[cfg(test)]
#[path = "true_peak_tests.rs"]
mod tests;
