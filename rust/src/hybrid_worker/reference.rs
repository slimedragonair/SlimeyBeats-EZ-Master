//! Validate serialized measurements before the shared engine indexes bands.
use super::request::Limits;
use anyhow::{Result, ensure};
use mastercraft::analysis::Analysis;

fn numbers(values: &[f64], count: usize) -> bool {
    values.len() == count && values.iter().all(|x| x.is_finite())
}

pub fn validate(value: &Analysis, limits: &Limits) -> Result<()> {
    ensure!(
        value.duration_s.is_finite()
            && value.duration_s > 0.
            && value.duration_s <= limits.max_duration_seconds
            && (8000..=384000).contains(&value.sample_rate)
            && (1..=2).contains(&value.channels),
        "Invalid analysis dimensions"
    );
    let samples = value.duration_s * value.sample_rate as f64 * value.channels as f64;
    ensure!(
        samples * 4. <= limits.max_pcm_bytes as f64 + 0.001,
        "Analysis exceeds PCM budget"
    );
    let metrics = &value.metrics;
    ensure!(
        [
            metrics.integrated_lufs,
            metrics.true_peak_dbtp,
            metrics.sample_peak_dbfs,
            metrics.lra_lu,
            metrics.rms_dbfs,
            metrics.crest_db,
            metrics.plr_db
        ]
        .iter()
        .all(|x| x.is_finite() && (-1000. ..=1000.).contains(x))
            && metrics.correlation.is_finite()
            && (-1.000001..=1.000001).contains(&metrics.correlation)
            && numbers(&metrics.dc_offset, value.channels)
            && metrics.clipped_samples as f64 <= samples.ceil(),
        "Invalid analysis metrics"
    );
    ensure!(
        numbers(&value.measured_db, 10)
            && numbers(&value.deviation_db, 10)
            && value.valid.len() == 10,
        "Analysis requires ten finite tonal bands and validity flags"
    );
    let spectrum = &value.spectrum;
    ensure!(
        numbers(&spectrum.freqs, 240)
            && spectrum.freqs.iter().all(|x| *x > 0.)
            && spectrum.freqs.windows(2).all(|p| p[0] < p[1])
            && numbers(&spectrum.db, 240)
            && numbers(&spectrum.octave_power, 10)
            && spectrum.octave_power.iter().all(|x| *x >= 0.)
            && spectrum
                .cutoff_hz
                .is_none_or(|v| v.is_finite() && v >= 0. && v <= value.sample_rate as f64 / 2.)
            && spectrum.quiet_hf_flatness.is_finite()
            && (0. ..=1.000001).contains(&spectrum.quiet_hf_flatness)
            && spectrum.sibilance_spike_fraction.is_finite()
            && (0. ..=1.).contains(&spectrum.sibilance_spike_fraction),
        "Invalid analysis spectrum"
    );
    ensure!(
        value.issues.iter().all(|issue| issue.severity.is_finite()),
        "Invalid analysis issue values"
    );
    Ok(())
}
