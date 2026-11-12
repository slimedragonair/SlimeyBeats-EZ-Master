use crate::{
    audio::Audio,
    dsp::{convolution, dynamics::HOP, oversampling},
    meters::{lin, percentile},
};

pub fn saturate(audio: &mut Audio, drive: f64, mix: f64, protect: &[f32]) {
    if drive <= 0. || mix <= 0. {
        return;
    }
    let sample: Vec<_> = audio
        .samples
        .iter()
        .step_by((audio.samples.len() / 200000).max(1))
        .map(|v| v.abs() as f64)
        .collect();
    let peak = percentile(&sample, 0.999).max(1e-8);
    let d = lin(drive);
    let mut wet = oversampling::nonlinear(audio, 2, |x| {
        ((d * (x / peak + 0.08)).tanh() - (d * 0.08).tanh()) / d.tanh() * peak
    });
    let means: Vec<_> = (0..audio.channels)
        .map(|ch| {
            wet.samples
                .iter()
                .skip(ch)
                .step_by(audio.channels)
                .map(|v| *v as f64)
                .sum::<f64>()
                / audio.frames() as f64
        })
        .collect();
    for frame in wet.samples.chunks_exact_mut(audio.channels) {
        for (ch, v) in frame.iter_mut().enumerate() {
            *v -= means[ch] as f32;
        }
    }
    let gain = audio.rms() / wet.rms().max(1e-20);
    for (i, (dry, w)) in audio.samples.iter_mut().zip(wet.samples).enumerate() {
        let m = mix * (1. - protect.get(i / audio.channels / HOP).copied().unwrap_or(0.) as f64);
        *dry += (m * (w as f64 * gain - *dry as f64)) as f32;
    }
}
pub fn soft_clip(audio: &mut Audio, drive: f64, ceiling: f64) {
    if drive <= 0.05 {
        return;
    }
    let knee = ceiling * lin(-drive);
    let span = (ceiling - knee).max(1e-8);
    *audio = oversampling::nonlinear(audio, 4, |x| {
        if x.abs() <= knee {
            x
        } else {
            let u = (x.abs() - knee) / span;
            x.signum() * (knee + span * u / (1. + u))
        }
    });
}
pub fn psycho_bass(audio: &mut Audio, mix_db: f64) {
    let source = convolution::band(audio, 35., 90.);
    let mut harmonic = oversampling::nonlinear(&source, 2, |x| (x * 3.).tanh() - x * 3.);
    super::biquad::highpass(&mut harmonic, 70.);
    let gain = lin(mix_db) as f32;
    for (s, h) in audio.samples.iter_mut().zip(harmonic.samples) {
        *s += gain * h;
    }
}
