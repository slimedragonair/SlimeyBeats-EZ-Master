use super::{
    stft::{self, Transform},
    voice_cleanup::NoiseRegion,
};
use crate::{audio::Audio, meters::lin};
use anyhow::{Result, ensure};

const FFT_SIZE: usize = 2048;
const MAX_PROFILE_FRAMES: usize = 192;

pub(super) fn reduce(
    input: &Audio,
    amount: f64,
    region: Option<&NoiseRegion>,
    cancel: &dyn Fn() -> bool,
) -> Result<(Audio, bool)> {
    let first = region.map_or(0, |r| (r.start_seconds * input.sample_rate as f64) as usize);
    let end = region
        .map_or(input.frames(), |r| {
            (r.end_seconds * input.sample_rate as f64) as usize
        })
        .min(input.frames());
    let step = ((end - first) / MAX_PROFILE_FRAMES).max(FFT_SIZE / 4);
    let mut centers = if region.is_some() {
        ensure!(
            end - first >= FFT_SIZE,
            "Noise-only selection must be at least one analysis window (about 43 ms at 48 kHz)"
        );
        (first + FFT_SIZE / 2..=end - FFT_SIZE / 2)
            .step_by(step)
            .collect::<Vec<_>>()
    } else {
        (first..end).step_by(step).collect::<Vec<_>>()
    };
    if region.is_none() {
        centers.sort_by(|&a, &b| {
            let energy = |center: usize| {
                let start = center.saturating_sub(FFT_SIZE / 2);
                let end = (center + FFT_SIZE / 2).min(input.frames());
                input.samples[start * input.channels..end * input.channels]
                    .iter()
                    .map(|&v| (v as f64).powi(2))
                    .sum::<f64>()
                    / (end - start).max(1) as f64
            };
            energy(a).total_cmp(&energy(b))
        });
        centers.truncate(centers.len().div_ceil(10).max(1));
    }
    ensure!(!centers.is_empty(), "Noise profile must contain audio");
    let mut plan = Transform::new(FFT_SIZE);
    let mut noise = vec![vec![0_f32; FFT_SIZE]; input.channels];
    for (ch, profile) in noise.iter_mut().enumerate() {
        for &center in &centers {
            ensure!(!cancel(), "Cancelled");
            for (p, bin) in profile.iter_mut().zip(plan.at(input, ch, center as isize)) {
                *p += bin.norm_sqr() / centers.len() as f32;
            }
        }
    }
    if region.is_none() {
        let lo = (300. * FFT_SIZE as f64 / input.sample_rate as f64).ceil() as usize;
        let hi = (8000_f64.min(input.sample_rate as f64 * 0.45) * FFT_SIZE as f64
            / input.sample_rate as f64)
            .floor() as usize;
        let powers = (lo.min(hi)..hi.max(lo).min(FFT_SIZE / 2))
            .map(|i| {
                noise.iter().map(|channel| channel[i] as f64).sum::<f64>() / input.channels as f64
            })
            .collect::<Vec<_>>();
        let count = powers.len().max(1) as f64;
        let arithmetic = powers.iter().sum::<f64>() / count;
        let geometric = (powers.iter().map(|&v| v.max(1e-20).ln()).sum::<f64>() / count).exp();
        if arithmetic > 1e-15 && geometric / arithmetic < 0.12 {
            return Ok((input.clone(), false));
        }
    }
    let floor = lin(-if region.is_some() { 18. } else { 9. } * amount) as f32;
    let audio = stft::render(input, FFT_SIZE, cancel, |ch, _, bins| {
        for (i, bin) in bins.iter_mut().enumerate() {
            let power = bin.norm_sqr();
            let gain = (1. - amount as f32 * noise[ch][i] / power.max(1e-20))
                .max(0.)
                .sqrt()
                .clamp(floor, 1.);
            *bin *= gain;
        }
    })?;
    Ok((audio, true))
}
