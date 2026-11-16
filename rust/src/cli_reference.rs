//! Bounded reference-analysis input for the CLI's existing reference DSP path.

use anyhow::{Context, Result, ensure};
use mastercraft::analysis::Analysis;
use std::{fs::File, io::Read, path::Path};

const MAX_BYTES: u64 = 16 * 1024;
const MAX_PCM_BYTES: f64 = (512 * 1024 * 1024) as f64;

pub fn load(path: &Path) -> Result<Analysis> {
    let metadata = std::fs::symlink_metadata(path).context("Read reference analysis metadata")?;
    ensure!(
        metadata.is_file(),
        "Reference analysis must be a regular file"
    );
    let file = File::open(path).context("Open reference analysis")?;
    ensure!(
        file.metadata()?.is_file(),
        "Reference analysis must be a regular file"
    );
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "Reference analysis exceeds 16 KiB"
    );
    let mut analysis: Analysis =
        serde_json::from_slice(&bytes).context("Invalid reference Analysis JSON")?;
    validate(&analysis)?;
    // These fields are unrelated to reference matching and may contain browser
    // display text. Keep only validated measurements for the native DSP path.
    analysis.issues.clear();
    analysis.suggested_profile = "neutral".into();
    Ok(analysis)
}

fn within(value: f64, low: f64, high: f64) -> bool {
    value.is_finite() && (low..=high).contains(&value)
}

fn vector(values: &[f64], size: usize, low: f64, high: f64) -> bool {
    values.len() == size && values.iter().all(|&value| within(value, low, high))
}

fn validate(value: &Analysis) -> Result<()> {
    ensure!(
        within(value.duration_s, f64::MIN_POSITIVE, 1800.)
            && (8000..=384000).contains(&value.sample_rate)
            && (1..=2).contains(&value.channels),
        "Invalid reference audio geometry"
    );
    let samples = value.duration_s * value.sample_rate as f64 * value.channels as f64;
    ensure!(
        samples * 4. <= MAX_PCM_BYTES,
        "Reference exceeds decoded audio bounds"
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
        .all(|&number| within(number, -300., 300.))
            && within(metrics.correlation, -1., 1.)
            && vector(&metrics.dc_offset, value.channels, -1e6, 1e6)
            && metrics.clipped_samples as f64 <= samples.ceil(),
        "Invalid reference metrics"
    );
    ensure!(
        vector(&value.measured_db, 10, -300., 300.)
            && vector(&value.deviation_db, 10, -300., 300.)
            && value.valid.len() == 10,
        "Reference requires ten finite tonal bands and ten validity flags"
    );
    let spectrum = &value.spectrum;
    let nyquist = value.sample_rate as f64 / 2.;
    ensure!(
        vector(&spectrum.freqs, 240, f64::MIN_POSITIVE, nyquist)
            && spectrum.freqs.windows(2).all(|pair| pair[0] < pair[1])
            && vector(&spectrum.db, 240, -300., 300.)
            && vector(&spectrum.octave_power, 10, 0., 1e12)
            && spectrum
                .cutoff_hz
                .is_none_or(|number| within(number, 0., nyquist))
            && within(spectrum.quiet_hf_flatness, 0., 1.000001)
            && within(spectrum.sibilance_spike_fraction, 0., 1.),
        "Invalid reference spectrum"
    );
    ensure!(
        spectrum
            .high_frequency
            .as_ref()
            .is_none_or(|summary| summary.valid(value.sample_rate, value.duration_s)),
        "Invalid reference high-frequency evidence"
    );
    Ok(())
}

#[cfg(test)]
#[path = "cli_reference_tests.rs"]
mod tests;
