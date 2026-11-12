use crate::{
    audio::Audio,
    dsp::{
        convolution,
        dynamics::{self, HOP},
    },
    meters::percentile,
};
use serde_json::{Value, json};

pub fn shape(audio: &mut Audio, attack: f64, sustain: f64) -> Value {
    let band = convolution::band(audio, 50., 160.);
    let level = dynamics::block_db(&band, 1.5);
    let fast = dynamics::smooth(&level, audio.sample_rate, 0.5, 20., false);
    let slow = dynamics::smooth(&level, audio.sample_rate, 30., 150., false);
    let floor = percentile(&fast, 0.05) - 6.;
    let mut targets = Vec::with_capacity(fast.len());
    for (&f, &s) in fast.iter().zip(&slow) {
        let activity = ((f - floor) / 10.).clamp(0., 1.);
        let transient = ((f - s) / 12.).clamp(0., 1.) * activity;
        targets.push(-attack * transient + sustain * (1. - transient) * activity);
    }
    let smooth = dynamics::moving_average(
        &targets,
        (audio.sample_rate as f64 * 0.002 / HOP as f64).max(1.) as usize,
    );
    for (i, frame) in audio.samples.chunks_exact_mut(audio.channels).enumerate() {
        let delta = (dynamics::gain_at(&smooth, i) - 1.) as f32;
        for (ch, v) in frame.iter_mut().enumerate() {
            *v += delta * band.samples[i * audio.channels + ch];
        }
    }
    json!({"attack_db":attack,"sustain_db":sustain})
}

pub fn duck(audio: &mut Audio, hits: &[usize], depth: f64) {
    if hits.is_empty() || depth <= 0. {
        return;
    }
    let rate = hits.len() as f64 / audio.duration();
    let depth = depth * (2. / rate.max(1e-6)).clamp(0.35, 1.);
    let mut targets = vec![0_f64; audio.frames().div_ceil(HOP)];
    // Convert the hold duration to blocks exactly once; the old implementation divided twice.
    let hold = (0.060 * audio.sample_rate as f64 / HOP as f64).ceil() as usize;
    for &hit in hits {
        let start = (hit / HOP).saturating_sub(1);
        let end = (start + hold).min(targets.len());
        targets[start..end].fill(depth);
    }
    let gr = dynamics::smooth(&targets, audio.sample_rate, 4., 90., false);
    let band = convolution::band(audio, 22., 60.);
    for (i, frame) in audio.samples.chunks_exact_mut(audio.channels).enumerate() {
        let delta = (dynamics::gain_at(&gr, i) - 1.) as f32;
        for (ch, s) in frame.iter_mut().enumerate() {
            *s += delta * band.samples[i * audio.channels + ch];
        }
    }
}
