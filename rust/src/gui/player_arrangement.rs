//! Arrangement integration extends the existing Player and ABSource transport.
use super::{Player, RealtimeInput};
use crate::{
    audio::Audio,
    studio::arrangement::{Arrangement, ArrangementPlayer, AssetLibrary, RenderOptions, ScopedEq},
};
use anyhow::Result;
use std::sync::Arc;

impl Player {
    pub fn load_arrangement(
        &mut self,
        before: Arc<Audio>,
        prepared: ArrangementPlayer,
    ) -> Result<()> {
        anyhow::ensure!(
            before.sample_rate == prepared.sample_rate() && before.channels == prepared.channels(),
            "Arrangement comparison must match project rate and channels"
        );
        anyhow::ensure!(
            prepared.position() == 0,
            "Load a new arrangement at frame zero"
        );
        self.load_inner(
            before,
            None,
            0.,
            0.,
            Some(RealtimeInput::Arrangement(Box::new(prepared))),
        )
    }
    pub fn set_arrangement(
        &mut self,
        document: &Arrangement,
        assets: &AssetLibrary,
        options: &RenderOptions,
    ) -> Result<bool> {
        let Some(mailbox) = &self.arrangement_sender else {
            return Ok(false);
        };
        mailbox.set(document, assets, options)?;
        self.frames = usize::try_from(document.duration_frames())?;
        self.duration = self.frames as f64 / document.sample_rate as f64;
        Ok(true)
    }
    pub fn replace_arrangement(
        &mut self,
        document: &Arrangement,
        assets: &AssetLibrary,
        options: &RenderOptions,
    ) -> Result<bool> {
        self.set_arrangement(document, assets, options)
    }
    pub fn has_live_arrangement(&self) -> bool {
        self.arrangement_sender.is_some()
    }
    pub fn arrangement_preparing(&self) -> bool {
        self.arrangement_sender
            .as_ref()
            .is_some_and(|mailbox| mailbox.preparing())
    }
    pub fn arrangement_error(&self) -> Option<String> {
        self.arrangement_sender
            .as_ref()
            .and_then(|mailbox| mailbox.error())
    }
    pub fn set_arrangement_lane_controls(
        &self,
        id: &str,
        gain_db: f64,
        pan: f64,
        mute: bool,
        solo: bool,
    ) -> Result<bool> {
        let Some(mailbox) = &self.arrangement_sender else {
            return Ok(false);
        };
        mailbox.update_lane(id, gain_db, pan, mute, solo)?;
        Ok(true)
    }
    pub fn set_arrangement_clip_gain(&self, id: &str, gain_db: f64) -> Result<bool> {
        let Some(mailbox) = &self.arrangement_sender else {
            return Ok(false);
        };
        mailbox.update_clip(id, gain_db)?;
        Ok(true)
    }
    pub fn set_arrangement_eq(&self, effect: &ScopedEq) -> Result<bool> {
        let Some(mailbox) = &self.arrangement_sender else {
            return Ok(false);
        };
        mailbox.update_eq(effect)?;
        Ok(true)
    }
    #[cfg(test)]
    pub(in crate::gui) fn take_test_arrangement_source(
        &mut self,
    ) -> Option<super::super::audio_source::ABSource> {
        self.test_arrangement_source.take()
    }
}
