use super::{AssetLibrary, EqScope, ScopedEq, sources};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_FRAME: u64 = 9_007_199_254_740_991;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct FrameRange {
    pub start_frame: u64,
    pub end_frame: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct Clip {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub source_id: String,
    pub start_frame: u64,
    pub source_start_frame: u64,
    /// Exclusive project end, not a source coordinate.
    pub end_frame: u64,
    #[serde(default)]
    pub gain_db: f64,
    #[serde(default)]
    pub fade_in_frames: u64,
    #[serde(default)]
    pub fade_out_frames: u64,
}
impl Clip {
    pub fn duration_frames(&self) -> u64 {
        self.end_frame.saturating_sub(self.start_frame)
    }
    pub fn source_end_frame(&self) -> Result<u64> {
        self.source_start_frame
            .checked_add(self.duration_frames())
            .context("Clip source coordinates overflow")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct Lane {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub gain_db: f64,
    #[serde(default)]
    pub pan: f64,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub solo: bool,
    #[serde(default)]
    pub clips: Vec<Clip>,
}
impl Lane {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            gain_db: 0.,
            pan: 0.,
            mute: false,
            solo: false,
            clips: vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct Arrangement {
    pub version: u32,
    pub sample_rate: u32,
    pub channels: usize,
    #[serde(default)]
    pub lanes: Vec<Lane>,
    #[serde(default)]
    pub eq: Vec<ScopedEq>,
}
impl Arrangement {
    pub fn new(sample_rate: u32, channels: usize) -> Self {
        Self {
            version: 1,
            sample_rate,
            channels,
            lanes: vec![],
            eq: vec![],
        }
    }
    pub fn duration_frames(&self) -> u64 {
        self.lanes
            .iter()
            .flat_map(|lane| &lane.clips)
            .map(|clip| clip.end_frame)
            .max()
            .unwrap_or(0)
    }
    pub fn clip(&self, id: &str) -> Result<&Clip> {
        self.lanes
            .iter()
            .flat_map(|lane| &lane.clips)
            .find(|clip| clip.id == id)
            .context("Arrangement clip ID is not present")
    }
    pub fn lane(&self, id: &str) -> Result<&Lane> {
        self.lanes
            .iter()
            .find(|lane| lane.id == id)
            .context("Arrangement lane ID is not present")
    }
    /// Metadata and source geometry only; the player separately scans used PCM once.
    pub fn validate(&self, assets: &AssetLibrary) -> Result<()> {
        super::budget::check(self)?;
        ensure!(
            self.version == 1
                && (8000..=384_000).contains(&self.sample_rate)
                && (1..=2).contains(&self.channels),
            "Unsupported arrangement version or layout"
        );
        let mut ids = BTreeSet::new();
        for lane in &self.lanes {
            identifier(&lane.id)?;
            ensure!(
                ids.insert(lane.id.as_str()),
                "Arrangement IDs must be unique"
            );
            ensure!(
                !lane.name.trim().is_empty()
                    && lane.name.len() <= 512
                    && !lane.name.chars().any(char::is_control),
                "Lane name must be nonempty plain text"
            );
            gain(lane.gain_db)?;
            ensure!(
                lane.pan.is_finite() && (-1. ..=1.).contains(&lane.pan),
                "Lane pan must be -1 to +1"
            );
            ensure!(
                self.channels == 2 || lane.pan == 0.,
                "Pan requires a stereo project"
            );
            for clip in &lane.clips {
                if let Some(name) = &clip.name {
                    ensure!(
                        !name.trim().is_empty()
                            && name.len() <= 512
                            && !name.chars().any(char::is_control),
                        "Clip name must be nonempty bounded plain text"
                    );
                }
                identifier(&clip.id)?;
                identifier(&clip.source_id)?;
                ensure!(
                    ids.insert(clip.id.as_str()),
                    "Arrangement IDs must be unique"
                );
                gain(clip.gain_db)?;
                ensure!(
                    clip.start_frame < clip.end_frame
                        && clip.end_frame <= MAX_FRAME
                        && clip.source_start_frame <= MAX_FRAME,
                    "Clip project bounds are invalid"
                );
                ensure!(
                    clip.fade_in_frames <= clip.duration_frames()
                        && clip.fade_out_frames <= clip.duration_frames(),
                    "Clip fades must fit within the clip"
                );
                let audio = assets
                    .get(&clip.source_id)
                    .context("Arrangement source ID is not present")?;
                sources::layout(audio, self.sample_rate, self.channels)?;
                ensure!(
                    clip.source_end_frame()? <= audio.frames() as u64,
                    "Clip exceeds its immutable source"
                );
            }
        }
        for effect in &self.eq {
            identifier(&effect.id)?;
            ensure!(
                ids.insert(effect.id.as_str()),
                "Arrangement IDs must be unique"
            );
            effect.validate_settings()?;
            let bounds = match &effect.scope {
                EqScope::Mix {} => FrameRange {
                    start_frame: 0,
                    end_frame: self.duration_frames(),
                },
                EqScope::Lane { lane_id } => {
                    self.lane(lane_id)?;
                    FrameRange {
                        start_frame: 0,
                        end_frame: self.duration_frames(),
                    }
                }
                EqScope::Clip { clip_id } => {
                    let clip = self.clip(clip_id)?;
                    FrameRange {
                        start_frame: 0,
                        end_frame: assets[&clip.source_id].frames() as u64,
                    }
                }
            };
            if let Some(range) = effect.range {
                ensure!(
                    range.start_frame < range.end_frame
                        && range.end_frame - range.start_frame
                            >= (self.sample_rate as f64 * 0.020).ceil() as u64
                        && range.start_frame >= bounds.start_frame
                        && range.end_frame <= bounds.end_frame,
                    "Scoped EQ range must fit project bounds, or source bounds for clip EQ"
                );
            }
        }
        Ok(())
    }
}
pub(crate) fn identifier(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 128
            && !id
                .chars()
                .any(|ch| ch.is_control() || ch == '/' || ch == '\\'),
        "Arrangement IDs must be nonempty bounded identifiers, not paths"
    );
    Ok(())
}
pub(super) fn gain(value: f64) -> Result<()> {
    ensure!(
        value.is_finite() && (-60. ..=12.).contains(&value),
        "Arrangement gain must be -60 to +12 dB"
    );
    Ok(())
}
