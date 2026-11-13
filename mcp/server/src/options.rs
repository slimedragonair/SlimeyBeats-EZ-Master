use anyhow::{Result, ensure};
use mastercraft::{
    config::{EqBand, Settings},
    profiles,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct MasterOptions {
    /// A bundled profile ID or a weighted blend such as rock:0.7+edm:0.3.
    pub profile: Option<String>,
    /// streaming_safe (default) is gentler; loud is a more forceful loudness direction.
    pub mode: Option<LoudnessMode>,
    /// Desired integrated loudness in LUFS, -40 to -5. Omit for the current/profile target.
    pub target_lufs: Option<f64>,
    /// Clear an explicit LUFS target and use the profile's target.
    pub use_profile_loudness: Option<bool>,
    /// Reconstructed peak ceiling in dBTP, -12 to 0; default -1.
    pub ceiling_dbtp: Option<f64>,
    /// Tonal correction strength, 0..1.5; 0 disables it, 1 is normal, about 0.6 is gentle.
    pub intensity: Option<f64>,
    /// Check punch/tonal regressions; default true.
    pub safety: Option<bool>,
    /// Makeup gain budget in dB, 0..40; default 18.
    pub max_makeup_db: Option<f64>,
    /// Limiter gain-reduction budget in dB, 0..12; default 6.
    pub max_limiter_gr_db: Option<f64>,
    /// Reference influence, 0..1.5; default 1. Load a reference first.
    pub match_strength: Option<f64>,
    /// Match a loaded reference's loudness; default false. Requires a reference.
    pub match_loudness: Option<bool>,
    /// Match stereo width when a reference is loaded; default true.
    pub match_width: Option<bool>,
    /// Advanced bypass IDs from get_capabilities.module_ids. Omit to retain safe defaults.
    pub disabled_modules: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoudnessMode {
    StreamingSafe,
    Loud,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Band {
    #[serde(rename = "type")]
    pub kind: FilterKind,
    /// Center/corner frequency in Hz, 5..20000.
    pub freq: f64,
    /// Boost/cut in dB, -18..18. Cut filters ignore gain.
    pub gain: f64,
    /// Band width/resonance, 0.1..18. Larger is narrower for a bell.
    pub q: f64,
    /// True enables the band; false bypasses it.
    pub on: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FilterKind {
    Bell,
    LowShelf,
    HighShelf,
    LowCut,
    HighCut,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct EqOptions {
    /// Exactly seven bands. Omit to retain the current bands.
    pub bands: Option<Vec<Band>>,
    /// Final manual gain in dB, -12..12. Peak protection still applies; not listening volume.
    pub output_gain_db: Option<f64>,
    /// Compensate EQ's level change subject to peak/gain limits; default true.
    pub hold_loudness: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Wav16,
    #[default]
    Wav24,
    Wav32f,
    Flac16,
    Flac24,
    Mp3,
}

impl ExportFormat {
    pub fn name(self) -> &'static str {
        match self {
            Self::Wav16 => "wav16",
            Self::Wav24 => "wav24",
            Self::Wav32f => "wav32f",
            Self::Flac16 => "flac16",
            Self::Flac24 => "flac24",
            Self::Mp3 => "mp3",
        }
    }
}

pub fn profile(name: &str) -> Result<serde_json::Value> {
    ensure!(
        !name.is_empty() && name.len() <= 256,
        "Choose a bundled profile ID"
    );
    let all = profiles::builtin();
    let parts = name.split('+').collect::<Vec<_>>();
    ensure!(parts.len() <= 4, "Blend at most four profiles");
    let blended = parts.len() > 1;
    for part in parts {
        let parts = part.trim().split(':').collect::<Vec<_>>();
        ensure!(
            parts.len() <= 2 && (blended || parts.len() == 1),
            "Use a bundled ID or a weighted blend"
        );
        let key = parts[0].trim();
        ensure!(
            all.contains_key(key),
            "Unknown bundled profile; use get_capabilities for IDs"
        );
        if let Some(weight) = parts.get(1) {
            let weight: f64 = weight.parse()?;
            ensure!(
                weight.is_finite() && weight > 0. && weight <= 1000.,
                "Blend weights must be positive finite numbers up to 1000"
            );
        }
    }
    profiles::resolve(name, std::path::Path::new(""))
}

impl MasterOptions {
    pub fn apply(&self, current: &Settings, has_reference: bool) -> Result<Settings> {
        let mut s = current.clone();
        if let Some(v) = &self.profile {
            profile(v)?;
            s.profile = v.clone();
        }
        if let Some(v) = self.mode {
            s.mode = match v {
                LoudnessMode::StreamingSafe => "streaming_safe",
                LoudnessMode::Loud => "loud",
            }
            .into();
        }
        if self.use_profile_loudness == Some(true) {
            s.target_lufs = None;
        }
        if let Some(v) = self.target_lufs {
            s.target_lufs = Some(v);
        }
        if let Some(v) = self.ceiling_dbtp {
            s.ceiling_dbtp = v;
        }
        if let Some(v) = self.intensity {
            s.intensity = v;
        }
        if let Some(v) = self.safety {
            s.safety = v;
        }
        if let Some(v) = self.max_makeup_db {
            s.max_makeup_db = v;
        }
        if let Some(v) = self.max_limiter_gr_db {
            s.max_limiter_gr_db = v;
        }
        if let Some(v) = self.match_strength {
            s.match_strength = v;
        }
        if let Some(v) = self.match_loudness {
            s.match_loudness = v;
        }
        if let Some(v) = self.match_width {
            s.match_width = v;
        }
        if let Some(v) = &self.disabled_modules {
            s.disabled = v.iter().cloned().collect();
        }
        ensure!(
            has_reference || !s.match_loudness,
            "Load a reference before matching its loudness"
        );
        s.validate()?;
        Ok(s)
    }
}

impl EqOptions {
    pub fn apply(&self, current: &Settings) -> Result<Settings> {
        let mut s = current.clone();
        if let Some(bands) = &self.bands {
            s.manual_eq = bands
                .iter()
                .map(|b| EqBand {
                    kind: match b.kind {
                        FilterKind::Bell => "bell",
                        FilterKind::LowShelf => "low_shelf",
                        FilterKind::HighShelf => "high_shelf",
                        FilterKind::LowCut => "low_cut",
                        FilterKind::HighCut => "high_cut",
                    }
                    .into(),
                    freq: b.freq,
                    gain: b.gain,
                    q: b.q,
                    on: b.on,
                })
                .collect();
        }
        if let Some(v) = self.output_gain_db {
            s.eq_output_db = v;
        }
        if let Some(v) = self.hold_loudness {
            s.eq_hold_loudness = v;
        }
        s.validate()?;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tool_options_reuse_native_validation_and_forbid_profile_paths() {
        let s = Settings::default();
        assert!(profile("../../private").is_err());
        assert!(profile("neutral+../../private").is_err());
        assert!(profile("neutral:0.7+edm:0.3").is_ok());
        let options = MasterOptions {
            ceiling_dbtp: Some(3.),
            ..Default::default()
        };
        assert!(options.apply(&s, false).is_err());
        let options = MasterOptions {
            match_loudness: Some(true),
            ..Default::default()
        };
        assert!(options.apply(&s, false).is_err());
        let eq = EqOptions {
            bands: Some(vec![]),
            ..Default::default()
        };
        assert!(eq.apply(&s).is_err());
        assert!(
            serde_json::from_value::<MasterOptions>(serde_json::json!({"out_dir":"outside"}))
                .is_err()
        );
    }
}
