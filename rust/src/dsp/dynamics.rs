use crate::{
    audio::Audio,
    dsp::convolution,
    meters::{db, lin, percentile},
};
use serde::Serialize;

pub const HOP: usize = 16;
pub fn block_db(audio: &Audio, window_ms: f64) -> Vec<f64> {
    let mut powers = Vec::with_capacity(audio.frames().div_ceil(HOP));
    for chunk in audio.samples.chunks(HOP * audio.channels) {
        let mut level = 0_f64;
        for ch in 0..audio.channels {
            let energy = chunk
                .iter()
                .skip(ch)
                .step_by(audio.channels)
                .map(|&v| (v as f64).powi(2))
                .sum::<f64>()
                / HOP as f64;
            level = level.max(energy);
        }
        powers.push(level);
    }
    let count = (window_ms * 0.001 * audio.sample_rate as f64 / HOP as f64).max(1.) as usize;
    moving_average(&powers, count.min(powers.len()))
        .into_iter()
        .map(|p| 10. * p.max(1e-20).log10())
        .collect()
}
pub fn moving_average(values: &[f64], count: usize) -> Vec<f64> {
    if count <= 1 {
        return values.to_vec();
    }
    let mut sum = vec![0.; values.len() + 1];
    for (i, v) in values.iter().enumerate() {
        sum[i + 1] = sum[i] + v;
    }
    (0..values.len())
        .map(|i| {
            let end = (i + count.div_ceil(2)).min(values.len());
            let start = (i + count.div_ceil(2)).saturating_sub(count);
            (sum[end] - sum[start]) / count as f64
        })
        .collect()
}
pub fn coefficient(ms: f64, sr: u32) -> f64 {
    (-(HOP as f64) / (ms.max(0.01) * 0.001 * sr as f64)).exp()
}
pub fn smooth(target: &[f64], sr: u32, attack: f64, release: f64, auto: bool) -> Vec<f64> {
    let (a, r, rf, rs, slow_a) = (
        coefficient(attack, sr),
        coefficient(release, sr),
        coefficient(release / 3., sr),
        coefficient(release * 2.5, sr),
        coefficient(400., sr),
    );
    let (mut state, mut slow) = (0_f64, 0_f64);
    target
        .iter()
        .map(|&t| {
            slow = slow_a * slow + (1. - slow_a) * t;
            let rel = if auto {
                let w = if state > 1e-6 {
                    (slow / state).clamp(0., 1.)
                } else {
                    1.
                };
                w * rs + (1. - w) * rf
            } else {
                r
            };
            let c = if t > state { a } else { rel };
            state = c * state + (1. - c) * t;
            state
        })
        .collect()
}
pub fn gain_at(gr: &[f64], frame: usize) -> f64 {
    if gr.is_empty() {
        return 1.;
    }
    let pos = (frame as f64 / HOP as f64 - 0.5).max(0.);
    let a = (pos as usize).min(gr.len() - 1);
    let b = (a + 1).min(gr.len() - 1);
    lin(-(gr[a] + (gr[b] - gr[a]) * pos.fract()))
}
pub fn gain_computer(level: f64, threshold: f64, ratio: f64, knee: f64) -> f64 {
    let over = level - threshold;
    let slope = 1. - 1. / ratio.max(1.0001);
    if knee > 0. && over.abs() < knee / 2. {
        slope * (over + knee / 2.).powi(2) / (2. * knee)
    } else {
        over.max(0.) * slope
    }
}
#[derive(Clone, Serialize, Debug)]
pub struct Compression {
    pub threshold_db: f64,
    pub ratio: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub max_gr_db: f64,
    pub auto_release: bool,
}
pub fn compress(audio: &mut Audio, cfg: &Compression) -> f64 {
    let levels = block_db(audio, 10.);
    let target: Vec<_> = levels
        .iter()
        .map(|&level| gain_computer(level, cfg.threshold_db, cfg.ratio, 6.).min(cfg.max_gr_db))
        .collect();
    let gr = smooth(
        &target,
        audio.sample_rate,
        cfg.attack_ms,
        cfg.release_ms,
        cfg.auto_release,
    );
    for (i, frame) in audio.samples.chunks_exact_mut(audio.channels).enumerate() {
        let gain = gain_at(&gr, i) as f32;
        for sample in frame {
            *sample *= gain;
        }
    }
    gr.iter().copied().fold(0_f64, f64::max)
}
pub fn auto_threshold(audio: &Audio, ratio: f64, amount: f64) -> f64 {
    percentile(&block_db(audio, 3000.), 0.95) - amount * ratio / ((ratio - 1.).max(0.01))
}

#[derive(Clone, Serialize, Debug)]
pub struct DynamicBand {
    pub lo: f64,
    pub hi: f64,
    pub max_cut: f64,
    pub ratio: f64,
    pub attack: f64,
    pub release: f64,
    pub over: f64,
    pub relative: bool,
}
pub fn dynamic_band(audio: &mut Audio, cfg: &DynamicBand, protect: &[f32]) -> f64 {
    if cfg.max_cut < 0.01 || cfg.lo >= audio.sample_rate as f64 * 0.45 {
        return 0.;
    }
    let band = convolution::band(audio, cfg.lo, cfg.hi);
    let lb = block_db(&band, 5.);
    let lf = block_db(audio, 5.);
    let floor = percentile(&lf, 0.99).max(-20.) - 40.;
    let detector: Vec<_> = lb
        .iter()
        .zip(&lf)
        .map(|(b, f)| if cfg.relative { b - f } else { *b })
        .collect();
    let active: Vec<_> = detector
        .iter()
        .zip(&lf)
        .filter(|(_, f)| **f > floor)
        .map(|(d, _)| *d)
        .collect();
    if active.is_empty() {
        return 0.;
    }
    let threshold = percentile(&active, 0.5) + cfg.over;
    let desired: Vec<_> = detector
        .iter()
        .enumerate()
        .map(|(i, &d)| {
            if lf[i] <= floor {
                0.
            } else {
                let protected = protect.get(i).copied().unwrap_or(0.) as f64;
                gain_computer(d, threshold, cfg.ratio, 3.).min(cfg.max_cut) * (1. - protected)
            }
        })
        .collect();
    let gr = smooth(&desired, audio.sample_rate, cfg.attack, cfg.release, false);
    for (i, frame) in audio.samples.chunks_exact_mut(audio.channels).enumerate() {
        let delta = (gain_at(&gr, i) - 1.) as f32;
        for (ch, s) in frame.iter_mut().enumerate() {
            *s += delta * band.samples[i * audio.channels + ch];
        }
    }
    gr.iter().copied().fold(0_f64, f64::max)
}

pub fn multiband(audio: &mut Audio, crossovers: &[f64], configs: &[Compression]) -> Vec<f64> {
    let mut previous = vec![0_f32; audio.samples.len()];
    let mut sum = vec![0_f32; audio.samples.len()];
    let mut reductions = Vec::new();
    for i in 0..=crossovers.len() {
        let lp = if i < crossovers.len() {
            convolution::lowpass(audio, crossovers[i])
        } else {
            audio.clone()
        };
        let mut band = lp.clone();
        for (sample, p) in band.samples.iter_mut().zip(&previous) {
            *sample -= p;
        }
        previous = lp.samples;
        if let Some(cfg) = configs.get(i) {
            let before = band.rms();
            let mut c = cfg.clone();
            c.threshold_db = auto_threshold(&band, c.ratio, c.max_gr_db * 0.6);
            reductions.push(compress(&mut band, &c));
            let makeup = db(before / band.rms().max(1e-20)).clamp(0., c.max_gr_db);
            band.gain(makeup);
        }
        for (s, v) in sum.iter_mut().zip(band.samples) {
            *s += v;
        }
    }
    audio.samples = sum;
    reductions
}
