pub mod playback_spectrum;
mod spectrum;
pub mod transients;
use crate::{
    audio::Audio,
    meters::{self, Metrics, percentile},
    profiles::{self, BANDS},
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub use spectrum::{Spectrum, spectrum};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Issue {
    pub key: String,
    pub severity: f64,
    pub detail: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Analysis {
    pub duration_s: f64,
    pub sample_rate: u32,
    pub channels: usize,
    pub metrics: Metrics,
    pub spectrum: Spectrum,
    pub measured_db: Vec<f64>,
    pub deviation_db: Vec<f64>,
    pub valid: Vec<bool>,
    pub issues: Vec<Issue>,
    pub suggested_profile: String,
}
impl Analysis {
    pub fn severity(&self, key: &str) -> f64 {
        self.issues
            .iter()
            .find(|x| x.key == key)
            .map(|x| x.severity)
            .unwrap_or(0.)
    }
}

pub fn analyze(audio: &Audio, profile: &Value) -> Result<Analysis> {
    let metrics = meters::measure(audio)?;
    let spec = spectrum(audio);
    let curve = profiles::target_curve(profile)?;
    let measured = spec.octaves();
    let valid: Vec<_> = BANDS
        .iter()
        .map(|&f| {
            f * 2_f64.sqrt() < audio.sample_rate as f64 * 0.49
                && f < spec.cutoff_hz.unwrap_or(f64::MAX)
        })
        .collect();
    let core: Vec<_> = (2..=8)
        .filter(|&i| valid[i])
        .map(|i| measured[i] - curve[i])
        .collect();
    let offset = percentile(&core, 0.5);
    let dev: Vec<_> = measured
        .iter()
        .zip(&curve)
        .map(|(m, t)| m - t - offset)
        .collect();
    let mut issues = Vec::new();
    let mut issue = |key: &str, severity: f64, detail: String| {
        issues.push(Issue {
            key: key.into(),
            severity: severity.clamp(0., 100.),
            detail,
        })
    };
    let mud = dev[3].max(dev[4]);
    issue(
        "mud",
        (mud - 2.).max(0.) * 15.,
        format!("Low-mid deviation {mud:+.1} dB against the selected tonal curve"),
    );
    let harsh = dev[6].max(dev[7]);
    issue(
        "harshness",
        (harsh - 2.).max(0.) * 15.,
        format!("Upper-mid deviation {harsh:+.1} dB; automatic corrections are heuristic"),
    );
    let sibilance = spec.sibilance_spike_fraction;
    issue(
        "sibilance",
        (sibilance - 0.08).max(0.) * 250.,
        format!(
            "{:.0}% of active frames contain high-frequency bursts",
            100. * sibilance
        ),
    );
    issue(
        "fizz",
        (dev[8] - 3.).max(0.) * 14.,
        format!("8 kHz deviation {:+.1} dB", dev[8]),
    );
    let clip = 100. * metrics.clipped_samples as f64 / audio.samples.len() as f64;
    issue(
        "clipping",
        if clip > 0. { 20. + clip * 60. } else { 0. },
        format!("{clip:.3}% of samples reach digital full scale"),
    );
    issue(
        "noise",
        if spec.noise_candidate { 45. } else { 0. },
        format!(
            "Quiet HF flatness {:.2}; spectral reduction requires quiet stationary hiss",
            spec.quiet_hf_flatness
        ),
    );
    issue(
        "stereo",
        if metrics.correlation < 0. { 60. } else { 0. },
        format!("Stereo correlation {:+.2}", metrics.correlation),
    );
    let suggested = profiles::builtin()
        .iter()
        .map(|(name, p)| {
            let target = profiles::target_curve(p).unwrap_or_else(|_| vec![0.; 10]);
            let diffs: Vec<_> = (2..=8)
                .filter(|&i| valid[i])
                .map(|i| measured[i] - target[i])
                .collect();
            let mean = percentile(&diffs, 0.5);
            let error = diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>();
            (name.clone(), error)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|v| v.0)
        .unwrap_or_else(|| "neutral".into());
    Ok(Analysis {
        duration_s: audio.duration(),
        sample_rate: audio.sample_rate,
        channels: audio.channels,
        metrics,
        spectrum: spec,
        measured_db: measured,
        deviation_db: dev,
        valid,
        issues,
        suggested_profile: suggested,
    })
}
