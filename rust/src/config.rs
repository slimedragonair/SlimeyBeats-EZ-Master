use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Settings {
    pub profile: String,
    pub mode: String,
    pub target_lufs: Option<f64>,
    pub ceiling_dbtp: f64,
    pub intensity: f64,
    pub formats: Vec<String>,
    pub output_sr: u32,
    pub out_dir: PathBuf,
    pub suffix: String,
    pub reference: Option<PathBuf>,
    pub match_strength: f64,
    pub match_loudness: bool,
    pub match_width: bool,
    pub dither: bool,
    pub safety: bool,
    pub max_makeup_db: f64,
    pub max_limiter_gr_db: f64,
    pub disabled: BTreeSet<String>,
    pub manual_eq: Vec<EqBand>,
    pub eq_output_db: f64,
    pub eq_hold_loudness: bool,
    pub modules: Value,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            profile: "neutral".into(),
            mode: "streaming_safe".into(),
            target_lufs: None,
            ceiling_dbtp: -1.0,
            intensity: 1.0,
            formats: vec!["wav24".into()],
            output_sr: 0,
            out_dir: PathBuf::from("output"),
            suffix: "_master".into(),
            reference: None,
            match_strength: 1.0,
            match_loudness: false,
            match_width: true,
            dither: true,
            safety: true,
            max_makeup_db: 18.0,
            max_limiter_gr_db: 6.0,
            disabled: BTreeSet::from(["hf_rebuild".into()]),
            manual_eq: default_eq(),
            eq_output_db: 0.0,
            eq_hold_loudness: true,
            modules: serde_json::json!({}),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct EqBand {
    #[serde(rename = "type")]
    pub kind: String,
    pub freq: f64,
    pub gain: f64,
    pub q: f64,
    pub on: bool,
}

pub fn default_eq() -> Vec<EqBand> {
    [50., 120., 300., 800., 2000., 5000., 10000.]
        .into_iter()
        .enumerate()
        .map(|(i, freq)| EqBand {
            kind: if i == 0 {
                "low_shelf"
            } else if i == 6 {
                "high_shelf"
            } else {
                "bell"
            }
            .into(),
            freq,
            gain: 0.,
            q: 0.707,
            on: true,
        })
        .collect()
}

impl Settings {
    pub fn import_eq_preset(&mut self, path: &Path) -> Result<()> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("Read EQ preset {}", path.display()))?;
        let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
        let mut next = self.clone();
        next.manual_eq = serde_json::from_value(value["bands"].clone())?;
        next.eq_output_db = value["out_gain_db"].as_f64().unwrap_or(0.);
        next.eq_hold_loudness = value["hold_loudness"].as_bool().unwrap_or(true);
        if value["bypass"].as_bool().unwrap_or(false) {
            for band in &mut next.manual_eq {
                band.on = false;
            }
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.manual_eq.len() == 7,
            "Manual EQ requires exactly seven bands"
        );
        ensure!(
            ["streaming_safe", "loud"].contains(&self.mode.as_str()),
            "Unknown mode: {}",
            self.mode
        );
        ensure!(
            self.intensity.is_finite() && (0.0..=1.5).contains(&self.intensity),
            "Intensity must be 0–1.5"
        );
        ensure!(
            self.ceiling_dbtp.is_finite() && (-12.0..=0.0).contains(&self.ceiling_dbtp),
            "Ceiling must be -12–0 dBTP"
        );
        ensure!(
            self.target_lufs
                .is_none_or(|x| x.is_finite() && (-40.0..=-5.0).contains(&x)),
            "LUFS target must be -40 to -5"
        );
        ensure!(
            self.output_sr == 0 || (8000..=192000).contains(&self.output_sr),
            "Output sample rate must be 0 or 8000–192000"
        );
        ensure!(
            !self.formats.is_empty() && self.formats.iter().all(|s| FORMATS.contains(&s.as_str())),
            "Select valid export formats"
        );
        ensure!(
            !self.suffix.contains(['/', '\\', ':']) && !self.suffix.contains(".."),
            "Suffix must be a file name suffix"
        );
        ensure!(
            self.match_strength.is_finite() && (0.0..=1.5).contains(&self.match_strength),
            "Match strength must be 0–1.5"
        );
        ensure!(
            (0.0..=40.0).contains(&self.max_makeup_db)
                && (0.0..=12.0).contains(&self.max_limiter_gr_db),
            "Invalid gain limits"
        );
        ensure!(
            self.eq_output_db.is_finite() && (-12.0..=12.0).contains(&self.eq_output_db),
            "EQ output must be -12–12 dB"
        );
        for band in &self.manual_eq {
            ensure!(
                ["bell", "low_shelf", "high_shelf", "low_cut", "high_cut"]
                    .contains(&band.kind.as_str()),
                "Unknown EQ type"
            );
            ensure!(
                (5.0..=20000.0).contains(&band.freq)
                    && (-18.0..=18.0).contains(&band.gain)
                    && (0.1..=18.0).contains(&band.q),
                "Invalid EQ band"
            );
        }
        for name in &self.disabled {
            ensure!(MODULES.contains(&name.as_str()), "Unknown module: {name}");
        }
        Ok(())
    }
    pub fn enabled(&self, module: &str) -> bool {
        !self.disabled.contains(module) && self.modules[module]["enabled"].as_bool().unwrap_or(true)
    }
    pub fn number(&self, module: &str, key: &str, default: f64) -> f64 {
        self.modules[module][key]
            .as_f64()
            .filter(|x| x.is_finite())
            .unwrap_or(default)
    }
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("Read settings {}", path.display()))?;
        let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
        let settings = if let Some(general) = value.get("general") {
            let mut general_clean = general.clone();
            if let Some(map) = general_clean.as_object_mut() {
                map.retain(|_, v| !v.is_null());
            }
            let mut s: Self = serde_json::from_value(general_clean)?;
            if let Some(modules) = value.get("modules") {
                s.modules = modules.clone();
            }
            if s.modules["hf_rebuild"]["enabled"].as_bool() == Some(true) {
                s.disabled.remove("hf_rebuild");
            }
            if let Some(x) = general["ceiling_dbtp"].as_f64() {
                s.ceiling_dbtp = x;
            }
            s.safety = value["safety"]["enabled"].as_bool().unwrap_or(true);
            s.max_makeup_db = s.number("limiter", "max_makeup_db", 18.0);
            s.max_limiter_gr_db = s.number("limiter", "max_gr_abort_db", 6.0);
            s.reference = general["reference"]
                .as_str()
                .filter(|x| !x.is_empty())
                .map(PathBuf::from);
            if s.formats.is_empty() {
                s.formats = vec![general["format"].as_str().unwrap_or("wav24").into()];
            }
            if s.out_dir.as_os_str().is_empty() {
                s.out_dir = PathBuf::from("output");
            }
            s
        } else {
            serde_json::from_value(value)?
        };
        settings.validate()?;
        Ok(settings)
    }
}

pub const FORMATS: &[&str] = &["wav24", "wav16", "wav32f", "flac24", "flac16", "mp3"];
pub const MODULES: &[&str] = &[
    "headroom",
    "declip",
    "cleanup",
    "denoise",
    "hf_rebuild",
    "static_eq",
    "resonance",
    "low_end",
    "low_punch",
    "dynamic_eq",
    "deesser",
    "multiband",
    "glue",
    "saturation",
    "stereo",
    "clipper",
    "reference_match",
    "limiter",
];
