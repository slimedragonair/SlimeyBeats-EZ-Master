use crate::{audio::Audio, meters::percentile, profiles::BANDS};
use rustfft::{FftPlanner, num_complex::Complex32};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spectrum {
    pub freqs: Vec<f64>,
    pub db: Vec<f64>,
    pub cutoff_hz: Option<f64>,
    pub quiet_hf_flatness: f64,
    pub noise_candidate: bool,
    pub sibilance_spike_fraction: f64,
    pub octave_power: Vec<f64>,
}
impl Spectrum {
    pub fn octaves(&self) -> Vec<f64> {
        let reference = 10. * self.octave_power[5].max(1e-20).log10();
        self.octave_power
            .iter()
            .map(|v| 10. * v.max(1e-20).log10() - reference)
            .collect()
    }
}

pub fn spectrum(audio: &Audio) -> Spectrum {
    let size = if audio.sample_rate > 50000 {
        8192
    } else {
        4096
    };
    let hop = size / 2;
    let total = (audio.frames().saturating_sub(size) / hop + 1).max(1);
    let count = total.min(3072);
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(size);
    let window: Vec<_> = (0..size)
        .map(|i| (0.5 - 0.5 * (2. * PI * i as f64 / size as f64).cos()) as f32)
        .collect();
    let scale = 2.
        / (size as f64
            * window.iter().map(|&w| (w as f64).powi(2)).sum::<f64>()
            * audio.channels as f64);
    let bins = size / 2 + 1;
    let mut average = vec![0_f64; bins];
    let mut quiet = vec![0_f64; bins];
    let mut buffer = vec![Complex32::default(); size];
    let mut scratch = vec![Complex32::default(); fft.get_inplace_scratch_len()];
    let mut records = Vec::new();
    // Two bounded passes let silence be excluded without retaining a bins × track-length matrix.
    let starts: Vec<_> = (0..count)
        .map(|i| {
            if count > 1 {
                i * (total - 1) / (count - 1) * hop
            } else {
                0
            }
        })
        .collect();
    let energies: Vec<_> = starts
        .iter()
        .map(|&start| {
            audio.samples
                [start * audio.channels..((start + size) * audio.channels).min(audio.samples.len())]
                .iter()
                .map(|&v| (v as f64).powi(2))
                .sum::<f64>()
                / (size * audio.channels) as f64
        })
        .collect();
    let active_floor = (percentile(&energies, 0.95) * 0.001).max(1e-10);
    let quiet_ceiling = percentile(&energies, 0.1);
    let (mut active_count, mut quiet_count) = (0, 0);
    for (frame, &start) in starts.iter().enumerate() {
        let mut power = vec![0_f64; bins];
        for ch in 0..audio.channels {
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
            for i in 0..bins {
                power[i] += buffer[i].norm_sqr() as f64 * scale;
            }
        }
        if energies[frame] >= active_floor {
            for (a, p) in average.iter_mut().zip(&power) {
                *a += p;
            }
            active_count += 1;
            let sum = |lo: f64, hi: f64| {
                power
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| {
                        let f = *i as f64 * audio.sample_rate as f64 / size as f64;
                        f >= lo && f < hi
                    })
                    .map(|(_, p)| p)
                    .sum::<f64>()
            };
            records.push(
                10. * (sum(5000., 10000.) / sum(500., 4000.).max(1e-20))
                    .max(1e-20)
                    .log10(),
            );
        }
        if energies[frame] <= quiet_ceiling {
            for (a, p) in quiet.iter_mut().zip(&power) {
                *a += p;
            }
            quiet_count += 1;
        }
    }
    average
        .iter_mut()
        .for_each(|v| *v /= active_count.max(1) as f64);
    quiet
        .iter_mut()
        .for_each(|v| *v /= quiet_count.max(1) as f64);
    let df = audio.sample_rate as f64 / size as f64;
    let band = |src: &[f64], lo: f64, hi: f64| {
        src.iter()
            .enumerate()
            .filter(|(i, _)| *i as f64 * df >= lo && (*i as f64 * df) < hi)
            .map(|(_, p)| p)
            .sum::<f64>()
    };
    let octave_power = BANDS
        .iter()
        .map(|&fc| band(&average, fc / 2_f64.sqrt(), fc * 2_f64.sqrt()))
        .collect();
    let mut freqs = Vec::with_capacity(240);
    let mut db = Vec::with_capacity(240);
    for i in 0..240 {
        let f = 20. * (20000_f64.min(audio.sample_rate as f64 * 0.49) / 20.).powf(i as f64 / 239.);
        let lo = ((f * 2_f64.powf(-1. / 12.) / df) as usize).min(bins - 1);
        let hi = (((f * 2_f64.powf(1. / 12.) / df) as usize) + 1).clamp(lo + 1, bins);
        let p = average[lo..hi].iter().sum::<f64>() / (hi - lo) as f64;
        freqs.push(f);
        db.push(10. * p.max(1e-20).log10());
    }
    let hf: Vec<_> = quiet
        .iter()
        .enumerate()
        .filter(|(i, _)| *i as f64 * df >= 6000. && (*i as f64 * df) < 12000.)
        .map(|(_, p)| p.max(1e-20))
        .collect();
    let flatness = if hf.is_empty() {
        0.
    } else {
        (hf.iter().map(|v| v.ln()).sum::<f64>() / hf.len() as f64).exp()
            / (hf.iter().sum::<f64>() / hf.len() as f64)
    };
    let quiet_ratio = band(&quiet, 6000., 12000.) / band(&quiet, 500., 12000.).max(1e-20);
    let noise_candidate =
        flatness > 0.65 && quiet_ratio > 0.2 && 10. * quiet_ceiling.max(1e-20).log10() < -35.;
    let median = percentile(&records, 0.5);
    let spike =
        records.iter().filter(|v| **v > median + 6.).count() as f64 / records.len().max(1) as f64;
    let reference: Vec<_> = average
        .iter()
        .enumerate()
        .filter(|(i, _)| (*i as f64 * df) >= 8000. && (*i as f64 * df) < 11000.)
        .map(|(_, p)| 10. * p.max(1e-20).log10())
        .collect();
    let floor = percentile(&reference, 0.5) - 35.;
    let mut run = 0;
    let mut cutoff = None;
    if audio.sample_rate > 30000 {
        for (i, p) in average.iter().enumerate().skip((12000. / df) as usize) {
            run = if 10. * p.max(1e-20).log10() < floor {
                run + 1
            } else {
                0
            };
            if run as f64 * df >= 750. {
                cutoff = Some((i - run + 1) as f64 * df);
                break;
            }
        }
    }
    Spectrum {
        freqs,
        db,
        cutoff_hz: cutoff,
        quiet_hf_flatness: flatness,
        noise_candidate,
        sibilance_spike_fraction: spike,
        octave_power,
    }
}
