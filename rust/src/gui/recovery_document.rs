//! Private recovery schema. Audio references index immutable attachments, never external files.
use super::recovery_eq::SavedEq;
use crate::{
    analysis::Analysis,
    config::Settings,
    report::Report,
    studio::{
        spectral_repair::RepairSelection,
        stem_mix::{MixSettings, StemKind},
        voice_cleanup::VoiceSettings,
    },
    studio_native::plugins::{PluginValue, Probe},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Document {
    pub version: u32,
    pub settings: Settings,
    pub tracks: Vec<SavedTrack>,
    pub selected: Option<usize>,
    pub tab: usize,
    pub selected_band: usize,
    pub volume: f32,
    pub match_levels: bool,
    pub position: f64,
    pub studio: SavedStudio,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedTrack {
    pub path: PathBuf,
    pub result: Option<SavedResult>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedResult {
    pub before: usize,
    pub raw: Option<usize>,
    pub after: Option<usize>,
    pub analysis: Analysis,
    pub raw_analysis: Option<Analysis>,
    pub eq: Option<SavedEq>,
    pub report: Option<Report>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedStudio {
    pub source: Option<SavedSource>,
    pub bank: Option<SavedBank>,
    pub current: Option<SavedRevision>,
    pub mix: MixSettings,
    pub mix_dirty: bool,
    pub monitoring_mix: bool,
    pub page: String,
    pub target: SavedTarget,
    pub voice: VoiceSettings,
    pub repair: RepairSelection,
    pub two_stems: bool,
    pub volume: f32,
    pub match_levels: bool,
    pub position: f64,
    pub output_dir: PathBuf,
    pub export_format: String,
    pub plugin_path: Option<PathBuf>,
    pub plugin_class: String,
    pub plugin_values: Vec<PluginValue>,
    pub plugin_probe: Option<Probe>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedSource {
    pub path: PathBuf,
    pub audio: usize,
    pub loudness: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedBank {
    pub stems: Vec<SavedStem>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedStem {
    pub kind: StemKind,
    pub name: String,
    pub audio: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedRevision {
    pub before: usize,
    pub audio: usize,
    pub bank: Option<SavedBank>,
    pub mix: MixSettings,
    pub label: String,
    pub notes: Vec<String>,
    pub before_lufs: f64,
    pub after_lufs: f64,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SavedTarget {
    Source,
    Preview,
    Stem(StemKind),
}
