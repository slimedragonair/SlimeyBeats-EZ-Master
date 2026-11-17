use super::{Arrangement, ArrangementPlayer, AssetLibrary};
use crate::audio::Audio;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct RenderOptions {
    pub output_gain_db: f64,
    pub protect_peaks: bool,
    pub ceiling_dbtp: f64,
    pub max_output_bytes: u64,
}
impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            output_gain_db: 0.,
            protect_peaks: true,
            ceiling_dbtp: -1.,
            max_output_bytes: 512 * 1024 * 1024,
        }
    }
}
impl RenderOptions {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.output_gain_db.is_finite() && (-24. ..=12.).contains(&self.output_gain_db),
            "Arrangement output gain must be -24 to +12 dB"
        );
        ensure!(
            self.ceiling_dbtp.is_finite() && (-12. ..=0.).contains(&self.ceiling_dbtp),
            "Arrangement ceiling must be -12 to 0 dBTP"
        );
        ensure!(
            self.max_output_bytes > 0,
            "Arrangement output budget must be positive"
        );
        Ok(())
    }
}
pub fn render(
    document: &Arrangement,
    assets: &AssetLibrary,
    options: &RenderOptions,
    cancel: &dyn Fn() -> bool,
) -> Result<Audio> {
    ensure!(!cancel(), "Cancelled");
    options.validate()?;
    document.validate(assets)?;
    let samples = document
        .duration_frames()
        .checked_mul(document.channels as u64)
        .context("Arrangement output size overflow")?;
    ensure!(samples > 0, "The arrangement has no audio clips to render");
    ensure!(
        samples
            .checked_mul(4)
            .is_some_and(|bytes| bytes <= options.max_output_bytes),
        "Arrangement render exceeds the configured decoded-output budget"
    );
    let count =
        usize::try_from(samples).context("Arrangement output does not fit this platform")?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(count)
        .context("Cannot allocate arrangement output")?;
    let mut player = ArrangementPlayer::new_cancellable(document, assets, options, cancel)?;
    while let Some(frame) = player.next_frame()? {
        if player.position().is_multiple_of(4096) {
            ensure!(!cancel(), "Cancelled");
        }
        output.extend_from_slice(&frame[..document.channels]);
    }
    ensure!(!cancel(), "Cancelled");
    Audio::new(output, document.sample_rate, document.channels)
}
