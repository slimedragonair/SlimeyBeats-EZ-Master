use crate::{
    audio::Audio,
    studio::arrangement::{Arrangement, EqScope, FrameRange, RenderOptions},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct AssetInfo {
    pub id: String,
    pub name: String,
    pub frames: u64,
    pub sample_rate: u32,
    pub channels: usize,
}

#[derive(Clone)]
pub struct OwnedAsset {
    pub id: String,
    pub name: String,
    pub audio: Arc<Audio>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SessionLimits {
    pub max_decoded_bytes: u64,
    pub history_capacity: usize,
}
impl Default for SessionLimits {
    fn default() -> Self {
        Self {
            max_decoded_bytes: 2 * 1024 * 1024 * 1024,
            history_capacity: 32,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryInfo {
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub epoch: String,
    pub revision: u64,
    pub arrangement: Arrangement,
    pub assets: Vec<AssetInfo>,
    pub history: HistoryInfo,
    pub render_options: RenderOptions,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct EqTarget {
    pub scope: EqScope,
    #[serde(default)]
    pub range: Option<FrameRange>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Waveform {
    pub peaks: Vec<[f32; 2]>,
    pub start_frame: u64,
    pub end_frame: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EqResponse {
    pub frequencies: Vec<f64>,
    pub gain_db: Vec<f64>,
}
