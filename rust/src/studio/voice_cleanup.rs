use super::voice_noise;
use crate::{
    audio::Audio,
    dsp::{
        biquad,
        dynamics::{self, DynamicBand},
        limiter,
    },
    meters,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoiseRegion {
    pub start_seconds: f64,
    pub end_seconds: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VoiceSettings {
    pub highpass_hz: f64,
    pub noise_reduction: f64,
    pub noise_region: Option<NoiseRegion>,
    pub deess: f64,
    pub gate_threshold_db: Option<f64>,
    pub output_gain_db: f64,
    pub hold_loudness: bool,
    pub protect_peaks: bool,
    pub ceiling_dbtp: f64,
}
impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            highpass_hz: 75.,
            noise_reduction: 0.3,
            noise_region: None,
            deess: 0.3,
            gate_threshold_db: None,
            output_gain_db: 0.,
            hold_loudness: true,
            protect_peaks: true,
            ceiling_dbtp: -1.,
        }
    }
}
impl VoiceSettings {
    pub fn validate(&self, input: &Audio) -> Result<()> {
        ensure!(
            [
                self.highpass_hz,
                self.noise_reduction,
                self.deess,
                self.output_gain_db,
                self.ceiling_dbtp
            ]
            .iter()
            .all(|v| v.is_finite()),
            "Voice cleanup values must be finite"
        );
        ensure!(
            self.highpass_hz == 0. || (20. ..=300.).contains(&self.highpass_hz),
            "Voice high-pass must be 0 (off) or 20–300 Hz"
        );
        ensure!(
            (0. ..=1.).contains(&self.noise_reduction) && (0. ..=1.).contains(&self.deess),
            "Noise reduction and de-essing must be 0–1"
        );
        ensure!(
            (-12. ..=12.).contains(&self.output_gain_db)
                && (-12. ..=0.).contains(&self.ceiling_dbtp),
            "Invalid voice output gain or ceiling"
        );
        if let Some(threshold) = self.gate_threshold_db {
            ensure!(
                threshold.is_finite() && (-80. ..=-15.).contains(&threshold),
                "Voice gate threshold must be -80 to -15 dBFS"
            );
        }
        if let Some(region) = &self.noise_region {
            ensure!(
                region.start_seconds.is_finite()
                    && region.end_seconds.is_finite()
                    && region.start_seconds >= 0.
                    && region.start_seconds < region.end_seconds
                    && region.end_seconds <= input.duration(),
                "Select a nonempty noise-only region within the track"
            );
        }
        Ok(())
    }
}

pub struct VoiceResult {
    pub audio: Audio,
    pub notes: Vec<String>,
    pub peak_trim_db: f64,
    pub deess_max_cut_db: f64,
}

pub fn render(
    input: &Audio,
    settings: &VoiceSettings,
    cancel: &dyn Fn() -> bool,
) -> Result<VoiceResult> {
    settings.validate(input)?;
    ensure!(!cancel(), "Cancelled");
    let mut audio = input.clone();
    let mut notes = Vec::new();
    if settings.highpass_hz > 0. {
        biquad::highpass(&mut audio, settings.highpass_hz);
    }
    ensure!(!cancel(), "Cancelled");
    if settings.noise_reduction > 0. {
        let (cleaned, applied) = voice_noise::reduce(
            &audio,
            settings.noise_reduction,
            settings.noise_region.as_ref(),
            cancel,
        )?;
        audio = cleaned;
        if !applied {
            notes.push("Automatic noise reduction skipped: quiet frames were too tonal to establish a stationary-noise profile. Select a noise-only region to learn it explicitly.".into());
        }
    }
    let deess_max_cut_db = if settings.deess > 0. {
        dynamics::dynamic_band(
            &mut audio,
            &DynamicBand {
                lo: 4000.,
                hi: 9000_f64.min(input.sample_rate as f64 * 0.46),
                max_cut: 6. * settings.deess,
                ratio: 4.,
                attack: 3.,
                release: 80.,
                over: 2.,
                relative: true,
            },
            &[],
        )
    } else {
        0.
    };
    ensure!(!cancel(), "Cancelled");
    if let Some(threshold) = settings.gate_threshold_db {
        gate(&mut audio, threshold, cancel)?;
    }
    if settings.hold_loudness {
        let original = meters::integrated(input)?;
        let cleaned = meters::integrated(&audio)?;
        if original > -60. && cleaned > -60. {
            audio.gain((original - cleaned).clamp(-3., 3.));
        }
    }
    audio.gain(settings.output_gain_db);
    let peak_trim_db = if settings.protect_peaks {
        limiter::guard(&mut audio, settings.ceiling_dbtp)?
    } else {
        0.
    };
    ensure!(!cancel(), "Cancelled");
    ensure!(
        audio.samples.iter().all(|v| v.is_finite()),
        "Voice cleanup produced non-finite audio"
    );
    notes.push("Voice cleanup reduces rumble, stationary noise and strong sibilance. It does not isolate a voice from a music mix; use a vocal stem or a voice recording and audition the result.".into());
    Ok(VoiceResult {
        audio,
        notes,
        peak_trim_db,
        deess_max_cut_db,
    })
}

fn gate(audio: &mut Audio, threshold: f64, cancel: &dyn Fn() -> bool) -> Result<()> {
    let attack = (-1. / (0.005 * audio.sample_rate as f64)).exp();
    let release = (-1. / (0.1 * audio.sample_rate as f64)).exp();
    let mut envelope = 0_f64;
    let mut gain = 1_f64;
    for (i, frame) in audio.samples.chunks_mut(audio.channels).enumerate() {
        if i % 16384 == 0 {
            ensure!(!cancel(), "Cancelled");
        }
        let level = frame
            .iter()
            .map(|v| (*v as f64).abs())
            .fold(0_f64, f64::max);
        let c = if level > envelope { attack } else { release };
        envelope = c * envelope + (1. - c) * level;
        let db = 20. * envelope.max(1e-12).log10();
        let target = meters::lin(((db - threshold) * 2.).clamp(-24., 0.));
        let c = if target > gain { attack } else { release };
        gain = c * gain + (1. - c) * target;
        for sample in frame {
            *sample *= gain as f32;
        }
    }
    Ok(())
}
