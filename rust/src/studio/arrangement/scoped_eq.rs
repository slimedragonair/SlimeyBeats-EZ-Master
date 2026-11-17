use super::FrameRange;
use crate::config::{EqBand, default_eq};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub enum EqScope {
    Mix {},
    Lane { lane_id: String },
    Clip { clip_id: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct ScopedEq {
    pub id: String,
    pub scope: EqScope,
    /// Mix/lane use project frames. Clip ranges use immutable source frames.
    #[serde(default)]
    pub range: Option<FrameRange>,
    #[cfg_attr(feature = "schemas", schemars(with = "Vec<EqBandSchema>"))]
    pub bands: Vec<EqBand>,
    #[serde(default)]
    pub output_gain_db: f64,
    #[serde(default)]
    pub bypass: bool,
}

#[cfg(feature = "schemas")]
#[derive(schemars::JsonSchema)]
#[allow(dead_code)]
struct EqBandSchema {
    #[serde(rename = "type")]
    kind: String,
    freq: f64,
    gain: f64,
    q: f64,
    on: bool,
}
impl ScopedEq {
    pub fn new(id: impl Into<String>, scope: EqScope) -> Self {
        Self {
            id: id.into(),
            scope,
            range: None,
            bands: default_eq(),
            output_gain_db: 0.,
            bypass: false,
        }
    }
    pub(super) fn validate_settings(&self) -> Result<()> {
        ensure!(
            self.bands.len() == 7,
            "Scoped EQ requires exactly seven bands"
        );
        ensure!(
            self.output_gain_db.is_finite() && (-24. ..=12.).contains(&self.output_gain_db),
            "Scoped EQ output gain must be -24 to +12 dB"
        );
        for band in &self.bands {
            ensure!(
                [band.freq, band.gain, band.q]
                    .iter()
                    .all(|value| value.is_finite()),
                "Scoped EQ values must be finite"
            );
            ensure!(
                ["bell", "low_shelf", "high_shelf", "low_cut", "high_cut"]
                    .contains(&band.kind.as_str()),
                "Unknown scoped EQ band type"
            );
            ensure!(
                (20. ..=20_000.).contains(&band.freq)
                    && (-18. ..=18.).contains(&band.gain)
                    && (0.1..=18.).contains(&band.q),
                "Scoped EQ band is outside its supported range"
            );
        }
        Ok(())
    }
}
