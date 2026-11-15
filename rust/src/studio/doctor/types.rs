use super::validation;
use crate::audio::Audio;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ops::Range;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub enum RepairAction {
    Declip,
    ReduceLowMids,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct RepairRequest {
    pub action: RepairAction,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub strength: f64,
}

impl RepairRequest {
    /// Validate parameters and frame layout without scanning the song on a UI
    /// thread. Rendering validates finite PCM during its cancellable copy.
    pub fn validate(&self, audio: &Audio) -> Result<()> {
        self.frame_range(audio).map(|_| ())
    }

    pub(super) fn frame_range(&self, audio: &Audio) -> Result<Range<usize>> {
        validation::layout(audio)?;
        ensure!(
            [self.start_seconds, self.end_seconds, self.strength]
                .iter()
                .all(|v| v.is_finite()),
            "Doctor repair values must be finite"
        );
        ensure!(
            (0. ..=1.).contains(&self.strength),
            "Doctor strength must be 0 to 1"
        );
        ensure!(
            self.start_seconds >= 0.
                && self.start_seconds < self.end_seconds
                && self.end_seconds <= audio.duration(),
            "Select a nonempty doctor region within the track"
        );
        let start = (self.start_seconds * audio.sample_rate as f64).round() as usize;
        let end = (self.end_seconds * audio.sample_rate as f64).round() as usize;
        ensure!(
            start < end && end <= audio.frames(),
            "Doctor region must select at least one complete frame after rounding"
        );
        Ok(start..end)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    Clipping,
    Silence,
    PossibleEnding,
    LowMidBuildup,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Issue {
    pub id: String,
    pub kind: IssueKind,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub severity: String,
    pub title: String,
    pub description: String,
    pub evidence: Value,
    pub repair: Option<RepairAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IssueReport {
    pub schema_version: u32,
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub channels: usize,
    pub thresholds: Value,
    pub issues: Vec<Issue>,
    pub truncated: bool,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct RepairResult {
    pub audio: Audio,
    pub notes: Vec<String>,
    /// Interleaved samples whose bits differ from the input.
    pub changed_samples: usize,
    /// Largest attenuation applied to a proposed reconstructed sample for local
    /// sample-peak safety. This is not a global gain or true-peak measurement.
    pub peak_trim_db: f64,
}
