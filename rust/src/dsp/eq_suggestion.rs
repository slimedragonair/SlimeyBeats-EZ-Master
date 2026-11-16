use crate::{
    analysis::{Analysis, high_frequency::HighFrequencySummary},
    config::{EqBand, default_eq},
    meters::percentile,
    profiles::{self, BANDS},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const PROFILE_WEIGHT: f64 = 0.25;
const LOCAL_SHAPE_WEIGHT: f64 = 0.15;
const MAX_MOVE_DB: f64 = 2.5;
const QUANTUM_DB: f64 = 0.25;
const MIN_DURATION_SECONDS: f64 = 1.;
const QUIET_LUFS: f64 = -69.5;
const MIN_VALID_CORE_BANDS: usize = 3;
const MAX_COMBINED_HIGH_CUT_DB: f64 = 3.;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EqSuggestion {
    pub bands: Vec<EqBand>,
    pub notes: Vec<String>,
    pub high_frequency: Option<HighFrequencySummary>,
}

fn interpolate(values: &[f64], valid: &[bool], frequency: f64) -> f64 {
    let mut available = (0..BANDS.len()).filter(|&i| valid[i]);
    let Some(mut previous) = available.next() else {
        return 0.;
    };
    if frequency <= BANDS[previous] {
        return values[previous];
    }
    for i in available {
        if frequency <= BANDS[i] {
            let t = (frequency / BANDS[previous]).log2() / (BANDS[i] / BANDS[previous]).log2();
            return values[previous] + (values[i] - values[previous]) * t;
        }
        previous = i;
    }
    values[previous]
}

/// Pure, bounded suggestions: identical measurements, profile, and version produce identical bands.
pub fn suggest(measurement: &Analysis, profile: &Value) -> Result<Vec<EqBand>> {
    Ok(proposal(measurement, profile)?.bands)
}

/// Additive explanation/evidence API; proposing never mutates audio or accepted EQ.
pub fn proposal(measurement: &Analysis, profile: &Value) -> Result<EqSuggestion> {
    ensure!(
        measurement.measured_db.len() == BANDS.len()
            && measurement.valid.len() == BANDS.len()
            && measurement.measured_db.iter().all(|v| v.is_finite()),
        "No reliable EQ suggestion: invalid tonal measurements."
    );
    ensure!(
        measurement.duration_s.is_finite()
            && measurement.duration_s >= MIN_DURATION_SECONDS
            && measurement.metrics.integrated_lufs.is_finite()
            && measurement.metrics.integrated_lufs > QUIET_LUFS,
        "No reliable EQ suggestion: use at least one second of audible music."
    );
    ensure!(
        measurement
            .spectrum
            .high_frequency
            .as_ref()
            .is_none_or(|highs| highs.valid(measurement.sample_rate, measurement.duration_s)),
        "No reliable EQ suggestion: invalid upper-treble evidence."
    );
    let target = profiles::target_curve(profile)?;
    let core: Vec<_> = (2..=8)
        .filter(|&i| measurement.valid[i])
        .map(|i| measurement.measured_db[i] - target[i])
        .collect();
    ensure!(
        core.len() >= MIN_VALID_CORE_BANDS,
        "No reliable EQ suggestion: too few usable frequency bands."
    );
    let offset = percentile(&core, 0.5);
    let mut bands = default_eq();
    for band in &mut bands {
        if band.freq * 2_f64.sqrt() >= measurement.sample_rate as f64 * 0.49
            || band.freq >= measurement.spectrum.cutoff_hz.unwrap_or(f64::MAX)
        {
            band.on = false;
            continue;
        }
        let measured = interpolate(&measurement.measured_db, &measurement.valid, band.freq);
        let residual =
            (measured - interpolate(&target, &[true; 10], band.freq) - offset).clamp(-8., 8.);
        let neighborhood =
            (interpolate(&measurement.measured_db, &measurement.valid, band.freq / 2.)
                + interpolate(&measurement.measured_db, &measurement.valid, band.freq * 2.))
                * 0.5;
        let local = (measured - neighborhood).clamp(-6., 6.);
        let correction = -PROFILE_WEIGHT * residual - LOCAL_SHAPE_WEIGHT * local;
        let upper = if band.freq <= 120. || band.freq >= 8000. {
            1.0
        } else {
            MAX_MOVE_DB
        };
        let bounded = if correction.abs() < 0.5 {
            0.
        } else {
            correction.clamp(-MAX_MOVE_DB, upper)
        };
        band.gain = (bounded / QUANTUM_DB).round() * QUANTUM_DB;
    }
    let high_frequency = measurement.spectrum.high_frequency.clone();
    let mut notes = vec!["Conservative, deterministic starting point with 25% tonal-profile influence. Inspect the curve, Apply it to audition, and adjust by ear.".into()];
    if let Some(highs) = &high_frequency {
        notes.push(highs.note());
        if highs.excess_candidate && bands[6].on {
            // One air-band move: do not add a second cut to an existing correction.
            bands[6].gain = bands[6].gain.min(highs.suggested_cut_db);
            bound_combined_high_cut(&mut bands, measurement.sample_rate);
            notes.push(format!("Start with a {:.2} dB high shelf at 10 kHz. The complete proposed curve is limited to 3 dB of upper-treble attenuation. A shelf affects all sound in that region and cannot restore missing drum detail.", bands[6].gain));
        }
        if highs.sampling_capped {
            notes.push("This is a bounded sample across the song; unsampled passages may differ. Frequency evidence does not identify a snare, hi-hat, or AI origin.".into());
        }
    } else {
        notes.push("The extra upper-treble check needs at least two seconds and enough usable bandwidth above 10 kHz. No additional high-frequency guard was proposed.".into());
    }
    Ok(EqSuggestion {
        bands,
        notes,
        high_frequency,
    })
}

fn bound_combined_high_cut(bands: &mut [EqBand], sample_rate: u32) {
    // Quantize toward zero while easing overlapping cuts. The bounded iteration
    // reaches a flat negative branch, so the final response cannot exceed the cap.
    for _ in 0..16 {
        let minimum = (0..=96)
            .map(|i| {
                let frequency = 10_000. * 2_f64.powf(i as f64 / 96.);
                if frequency >= sample_rate as f64 * 0.49 {
                    return 0.;
                }
                bands
                    .iter()
                    .filter(|band| band.on)
                    .map(|band| {
                        super::biquad::from_band(band, sample_rate)
                            .response_db(frequency, sample_rate)
                    })
                    .sum::<f64>()
            })
            .fold(0., f64::min);
        if minimum >= -MAX_COMBINED_HIGH_CUT_DB - 1e-6 {
            break;
        }
        for band in bands.iter_mut().filter(|band| band.gain < 0.) {
            band.gain = (band.gain * 0.85 / QUANTUM_DB).ceil() * QUANTUM_DB;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analysis, audio::Audio};
    use std::{f64::consts::TAU, path::Path};
    fn measurement() -> (Analysis, Value) {
        let p = profiles::resolve("neutral", Path::new(".")).unwrap();
        let samples = (0..48000)
            .map(|i| {
                let t = i as f64 / 48000.;
                ([55., 140., 350., 900., 2500., 5500.]
                    .iter()
                    .map(|f| 0.02 * (TAU * f * t).sin())
                    .sum::<f64>()) as f32
            })
            .collect();
        (
            analysis::analyze(&Audio::new(samples, 48000, 1).unwrap(), &p).unwrap(),
            p,
        )
    }
    #[test]
    fn identical_input_and_profile_give_byte_identical_bands() {
        let (a, p) = measurement();
        let first = suggest(&a, &p).unwrap();
        let second = suggest(&a, &p).unwrap();
        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&second).unwrap()
        );
        assert_eq!(first.len(), 7);
        assert!(first.iter().all(|b| b.gain >= -2.5 && b.gain <= 2.5));
    }
    #[test]
    fn profile_is_a_light_bounded_influence() {
        let (mut a, mut p) = measurement();
        a.measured_db = vec![0.; 10];
        a.valid = vec![true; 10];
        p["tonal_db"] = serde_json::json!([0., 0., 0., 0., 0., 0., 0., 0., 0., 0.]);
        let flat = suggest(&a, &p).unwrap();
        p["tonal_db"][6] = serde_json::json!(4.);
        let shaped = suggest(&a, &p).unwrap();
        assert_eq!(flat[4].gain, 0.);
        assert_eq!(shaped[4].gain, 1.);
        assert!(
            shaped
                .iter()
                .zip(flat)
                .all(|(x, y)| (x.gain - y.gain).abs() <= 1.)
        );
    }
    #[test]
    fn unavailable_high_bands_are_disabled() {
        let (mut a, p) = measurement();
        a.sample_rate = 8000;
        let bands = suggest(&a, &p).unwrap();
        assert!(!bands[5].on && !bands[6].on);
        assert_eq!(bands[5].gain, 0.);
    }

    #[test]
    fn silence_short_and_missing_measurements_do_not_propose_corrections() {
        let (mut a, p) = measurement();
        a.metrics.integrated_lufs = -70.;
        assert!(suggest(&a, &p).is_err());
        a.metrics.integrated_lufs = -20.;
        a.duration_s = 0.2;
        assert!(suggest(&a, &p).is_err());
        a.duration_s = 1.;
        a.valid.fill(false);
        assert!(suggest(&a, &p).is_err());
        a.valid.fill(true);
        a.measured_db[2] = f64::NAN;
        assert!(suggest(&a, &p).is_err());
    }
}
