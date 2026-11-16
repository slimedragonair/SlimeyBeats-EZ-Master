use anyhow::{Result, ensure};
use mastercraft::{
    analysis::Analysis,
    studio::{
        doctor::RepairRequest,
        region_edit::RegionEdit,
        spectral_repair::RepairSelection,
        stem_mix::{MixSettings, StemKind},
        voice_cleanup::VoiceSettings,
    },
    studio_native::inference::SeparationMode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Import,
    Analyze,
    Master,
    Render,
    Issues,
    Repair,
    VoiceCleanup,
    SpectralRepair,
    Separate,
    Mix,
    ImportStems,
    Spectrogram,
    RegionEdit,
    Suggest,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StemInput {
    pub path: PathBuf,
    pub kind: StemKind,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ImportInput {
    Path(PathBuf),
    File(ImportFile),
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportFile {
    pub path: PathBuf,
    pub kind: Option<StemKind>,
    pub name: Option<String>,
}
impl ImportInput {
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::Path(path) => path,
            Self::File(file) => &file.path,
        }
    }
    pub fn kind(&self) -> Option<StemKind> {
        match self {
            Self::Path(_) => None,
            Self::File(file) => file.kind,
        }
    }
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Path(_) => None,
            Self::File(file) => file.name.as_deref(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub python: PathBuf,
    pub models: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub max_pcm_bytes: usize,
    pub max_bank_pcm_bytes: usize,
    pub max_duration_seconds: f64,
    pub decode_timeout_seconds: u64,
    pub waveform_bins: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_pcm_bytes: 2 * 1024 * 1024 * 1024,
            max_bank_pcm_bytes: 4 * 1024 * 1024 * 1024,
            max_duration_seconds: 3600.,
            decode_timeout_seconds: 3600,
            waveform_bins: 1024,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.max_pcm_bytes >= 4 && self.max_pcm_bytes <= isize::MAX as usize,
            "Invalid decoded PCM budget"
        );
        ensure!(
            self.max_bank_pcm_bytes >= 8 && self.max_bank_pcm_bytes <= isize::MAX as usize,
            "Invalid decoded bank budget"
        );
        ensure!(
            self.max_duration_seconds.is_finite() && self.max_duration_seconds > 0.,
            "Duration budget must be finite and positive"
        );
        ensure!(
            (1..=86400).contains(&self.decode_timeout_seconds),
            "Decode timeout must be 1 to 86400 seconds"
        );
        ensure!(
            (1..=1024).contains(&self.waveform_bins),
            "Waveform bins must be 1 to 1024"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SpectrogramSettings {
    pub columns: usize,
    pub rows: usize,
}
impl Default for SpectrogramSettings {
    fn default() -> Self {
        Self {
            columns: 320,
            rows: 128,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub api_version: u32,
    pub project_root: PathBuf,
    pub operation: Operation,
    pub input: Option<PathBuf>,
    pub out_dir: PathBuf,
    pub settings: Value,
    pub reference_analysis: Option<Analysis>,
    pub voice: Option<VoiceSettings>,
    pub repair: Option<RepairRequest>,
    pub spectral: Option<RepairSelection>,
    pub region: Option<RegionEdit>,
    pub spectrogram: Option<SpectrogramSettings>,
    pub stems: Option<Vec<StemInput>>,
    pub inputs: Option<Vec<ImportInput>>,
    pub mix: Option<MixSettings>,
    pub separation_mode: Option<SeparationMode>,
    pub runtime: Option<Runtime>,
    #[serde(default)]
    pub limits: Limits,
}

impl Request {
    pub fn validate(&self) -> Result<()> {
        use Operation::*;
        ensure!(self.api_version == 2, "Unsupported worker API version");
        self.limits.validate()?;
        for (present, expected, field) in [
            (self.voice.is_some(), VoiceCleanup, "voice"),
            (self.repair.is_some(), Repair, "repair"),
            (self.spectral.is_some(), SpectralRepair, "spectral"),
            (self.region.is_some(), RegionEdit, "region"),
        ] {
            ensure!(
                present == (self.operation == expected),
                "Missing or inapplicable {field} field"
            );
        }
        ensure!(
            self.reference_analysis.is_none() || self.operation == Master,
            "reference_analysis is master-only"
        );
        ensure!(
            self.spectrogram.is_none() || self.operation == Spectrogram,
            "spectrogram settings are spectrogram-only"
        );
        ensure!(
            self.mix.is_none() || self.operation == Mix,
            "mix settings are mix-only"
        );
        ensure!(
            self.inputs.is_none() || self.operation == ImportStems,
            "inputs are import_stems-only"
        );
        ensure!(
            self.stems.is_none() || matches!(self.operation, Mix | ImportStems),
            "stems are mix/import_stems-only"
        );
        ensure!(
            self.runtime.is_some() == (self.operation == Separate),
            "runtime is required for separation only"
        );
        ensure!(
            self.separation_mode.is_none() || self.operation == Separate,
            "separation_mode is separation-only"
        );
        if self.operation == Mix {
            ensure!(
                self.input.is_none() && self.stems.is_some(),
                "mix requires stems and no input"
            );
        } else if self.operation == ImportStems {
            let sources = usize::from(self.input.is_some())
                + usize::from(self.inputs.is_some())
                + usize::from(self.stems.is_some());
            ensure!(
                sources == 1,
                "import_stems requires exactly one of input ZIP, inputs, or stems"
            );
        } else {
            ensure!(self.input.is_some(), "Operation requires input");
        }
        if let Some(display) = &self.spectrogram {
            ensure!(
                (16..=320).contains(&display.columns) && (16..=128).contains(&display.rows),
                "Spectrogram supports 16 to 320 columns and 16 to 128 rows"
            );
        }
        Ok(())
    }
}
