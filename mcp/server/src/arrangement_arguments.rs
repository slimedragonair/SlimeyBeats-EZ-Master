use crate::options::ExportFormat;
use mastercraft::studio::{
    arrangement::{Edit, FrameRange, ScopedEq},
    arrangement_session::EqTarget,
};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateArrangement {
    #[serde(default = "rate")]
    pub sample_rate: u32,
    #[serde(default = "channels")]
    pub channels: usize,
    /// Required only to replace an existing workspace. Discover with get_arrangement.
    pub replace_session_id: Option<String>,
}
fn rate() -> u32 {
    48000
}
fn channels() -> usize {
    2
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArrangementRevision {
    pub session_id: String,
    pub expected_revision: u64,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArrangementSource {
    /// Explicit loaded source/stem/Studio audio ID from get_studio_session. Never a path or inferred filename.
    pub audio_id: String,
    pub name: Option<String>,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddArrangementAssets {
    pub session_id: String,
    pub expected_revision: u64,
    pub sources: Vec<ArrangementSource>,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditArrangement {
    pub session_id: String,
    pub expected_revision: u64,
    /// Atomic shared-core Edit batch; one Undo. Frames are nonnegative integers, ends exclusive.
    /// Clip EQ ranges are SOURCE frames; mix/lane ranges are PROJECT frames.
    pub edits: Vec<Edit>,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArrangementHistoryRequest {
    pub session_id: String,
    pub expected_revision: u64,
    #[serde(default)]
    pub redo: bool,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArrangementWaveform {
    pub session_id: String,
    pub expected_revision: u64,
    pub asset_id: String,
    /// Immutable SOURCE frames; end exclusive.
    pub range: FrameRange,
    pub bins: usize,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArrangementEqResponse {
    pub session_id: String,
    pub expected_revision: u64,
    pub effect: ScopedEq,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SuggestArrangementEq {
    pub session_id: String,
    pub expected_revision: u64,
    /// Explicit mix, lane_id or clip_id. Clip range is SOURCE frames; lane/mix PROJECT frames.
    pub target: EqTarget,
    /// Exclude ONLY the saved effect being replaced; retain all unrelated routed FX.
    pub exclude_eq_id: Option<String>,
    #[serde(default = "profile")]
    pub profile: String,
}
fn profile() -> String {
    "neutral".into()
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportArrangement {
    pub session_id: String,
    pub expected_revision: u64,
    /// New output filename stem; no directory. This is explicit export.
    pub name: String,
    #[serde(default)]
    pub format: ExportFormat,
    /// Zero preserves project rate; nonzero output resampling requires existing codec support.
    #[serde(default)]
    pub sample_rate: u32,
    #[serde(default)]
    pub dither: bool,
}
