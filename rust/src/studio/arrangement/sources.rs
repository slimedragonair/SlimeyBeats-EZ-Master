use crate::audio::Audio;
use anyhow::{Result, ensure};
use std::{collections::BTreeMap, sync::Arc};

/// Documents and history retain source IDs, never copies of source PCM.
pub type AssetLibrary = BTreeMap<String, Arc<Audio>>;

/// Pointer identity is valid here because an admitted Arc<Audio> stays immutable.
#[derive(Default)]
pub struct AssetValidationCache {
    pub(super) checked: AssetLibrary,
}
impl AssetValidationCache {
    pub fn new() -> Self {
        Self::default()
    }
    pub(super) fn validate_used(
        &mut self,
        document: &super::Arrangement,
        assets: &AssetLibrary,
        cancel: &dyn Fn() -> bool,
    ) -> Result<()> {
        document.validate(assets)?;
        for clip in document.lanes.iter().flat_map(|lane| &lane.clips) {
            let source = &assets[&clip.source_id];
            if self
                .checked
                .get(&clip.source_id)
                .is_some_and(|old| Arc::ptr_eq(old, source))
            {
                continue;
            }
            for chunk in source.samples.chunks(16_384) {
                ensure!(!cancel(), "Cancelled");
                ensure!(
                    chunk.iter().all(|value| value.is_finite()),
                    "Arrangement source contains nonfinite PCM"
                );
            }
            self.checked.insert(clip.source_id.clone(), source.clone());
        }
        self.checked.retain(|id, source| {
            assets
                .get(id)
                .is_some_and(|current| Arc::ptr_eq(current, source))
        });
        Ok(())
    }
}

pub(super) fn layout(audio: &Audio, rate: u32, channels: usize) -> Result<()> {
    ensure!(
        audio.sample_rate == rate,
        "Arrangement source rate differs; conform a derivative explicitly before import"
    );
    ensure!(
        audio.channels == channels || (audio.channels == 1 && channels == 2),
        "Arrangement source channels differ; stereo-to-mono requires an explicit downmix"
    );
    ensure!(
        audio.channels > 0
            && !audio.samples.is_empty()
            && audio.samples.len().is_multiple_of(audio.channels),
        "Arrangement source has invalid or incomplete PCM frames"
    );
    Ok(())
}
