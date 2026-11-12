use crate::audio::Audio;
use anyhow::Result;
use ebur128::{EbuR128, Mode};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Metrics {
    pub integrated_lufs: f64,
    pub true_peak_dbtp: f64,
    pub sample_peak_dbfs: f64,
    pub lra_lu: f64,
    pub rms_dbfs: f64,
    pub crest_db: f64,
    pub plr_db: f64,
    pub correlation: f64,
    pub dc_offset: Vec<f64>,
    pub clipped_samples: usize,
}

pub fn db(x: f64) -> f64 {
    20.0 * x.max(1e-12).log10()
}
pub fn lin(db: f64) -> f64 {
    10_f64.powf(db / 20.0)
}
pub fn integrated(audio: &Audio) -> Result<f64> {
    let mut meter = EbuR128::new(audio.channels as u32, audio.sample_rate, Mode::I)?;
    meter.add_frames_f32(&audio.samples)?;
    let value = meter.loudness_global()?;
    Ok(if value.is_finite() {
        value.max(-70.0)
    } else {
        -70.0
    })
}

pub fn measure(audio: &Audio) -> Result<Metrics> {
    let mut meter = EbuR128::new(
        audio.channels as u32,
        audio.sample_rate,
        Mode::I | Mode::LRA | Mode::TRUE_PEAK,
    )?;
    meter.add_frames_f32(&audio.samples)?;
    let loudness = meter.loudness_global()?;
    let lufs = if loudness.is_finite() {
        loudness.max(-70.0)
    } else {
        -70.0
    };
    let mut peak = 0_f64;
    for ch in 0..audio.channels {
        peak = peak.max(meter.true_peak(ch as u32)?);
    }
    let sample_peak = audio
        .samples
        .iter()
        .map(|x| x.abs() as f64)
        .fold(0_f64, f64::max);
    peak = crate::dsp::true_peak::peak(audio, peak.max(sample_peak));
    let rms = db(audio.rms());
    let mut dc = vec![0.0; audio.channels];
    let (mut ll, mut rr, mut lr) = (0.0, 0.0, 0.0);
    for frame in audio.samples.chunks_exact(audio.channels) {
        for (ch, &s) in frame.iter().enumerate() {
            dc[ch] += s as f64;
        }
        if audio.channels == 2 {
            ll += (frame[0] as f64).powi(2);
            rr += (frame[1] as f64).powi(2);
            lr += frame[0] as f64 * frame[1] as f64;
        }
    }
    for x in &mut dc {
        *x /= audio.frames() as f64;
    }
    Ok(Metrics {
        integrated_lufs: lufs,
        true_peak_dbtp: db(peak.max(sample_peak)),
        sample_peak_dbfs: db(sample_peak),
        lra_lu: meter.loudness_range().unwrap_or(0.0),
        rms_dbfs: rms,
        crest_db: db(sample_peak) - rms,
        plr_db: db(peak.max(sample_peak)) - lufs,
        correlation: if ll * rr > 1e-20 {
            (lr / (ll * rr).sqrt()).clamp(-1., 1.)
        } else {
            1.
        },
        dc_offset: dc,
        clipped_samples: audio.samples.iter().filter(|v| v.abs() >= 0.9999).count(),
    })
}

pub fn percentile(values: &[f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable_by(f64::total_cmp);
    let position = fraction.clamp(0., 1.) * (sorted.len() - 1) as f64;
    let i = position.floor() as usize;
    sorted[i] + (sorted[position.ceil() as usize] - sorted[i]) * position.fract()
}
