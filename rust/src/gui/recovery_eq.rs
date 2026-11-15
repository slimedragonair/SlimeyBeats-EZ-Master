//! Exact prepared EQ coefficients are private data, validated before playback.
use crate::dsp::{biquad::Biquad, eq_chain::EqParameters};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedEq {
    coefficients: [([f64; 3], [f64; 3]); 7],
    enabled: [bool; 7],
    gain_db: f64,
    ceiling_dbtp: f64,
    post_gain_db: f64,
}
impl From<EqParameters> for SavedEq {
    fn from(value: EqParameters) -> Self {
        Self {
            coefficients: value.coefficients.map(|c| (c.b, c.a)),
            enabled: value.enabled,
            gain_db: value.gain_db,
            ceiling_dbtp: value.ceiling_dbtp,
            post_gain_db: value.post_gain_db,
        }
    }
}
impl SavedEq {
    pub fn restore(&self) -> Result<EqParameters> {
        ensure!(
            [self.gain_db, self.ceiling_dbtp, self.post_gain_db]
                .iter()
                .all(|v| v.is_finite()),
            "Invalid recovery EQ gains"
        );
        ensure!(
            (-30.0..=30.0).contains(&self.gain_db)
                && (-12.0..=0.0).contains(&self.ceiling_dbtp)
                && self.post_gain_db <= 0.0,
            "Invalid recovery EQ bounds"
        );
        for (b, a) in &self.coefficients {
            ensure!(
                b.iter().chain(a).all(|v| v.is_finite()) && a[0] == 1.0,
                "Invalid recovery EQ coefficients"
            );
            // Jury stability conditions for a normalized real second-order denominator.
            ensure!(
                a[2].abs() < 1.0 && 1.0 + a[1] + a[2] > 0.0 && 1.0 - a[1] + a[2] > 0.0,
                "Unstable recovery EQ filter"
            );
        }
        Ok(EqParameters {
            coefficients: self.coefficients.map(|(b, a)| Biquad { b, a }),
            enabled: self.enabled,
            gain_db: self.gain_db,
            ceiling_dbtp: self.ceiling_dbtp,
            post_gain_db: self.post_gain_db,
        })
    }
}
