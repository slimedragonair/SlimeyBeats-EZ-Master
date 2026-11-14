//! Faders update the running source; materializing a preview remains explicit for saving.
use super::state::Studio;
use std::sync::atomic::Ordering;

impl Studio {
    pub(super) fn monitor_mix_changes(&mut self) {
        self.mix_dirty = true;
        let Some(bank) = &self.bank else {
            return;
        };
        match self.player.set_mix(bank, &self.mix) {
            Ok(true) => {
                self.monitoring_mix = true;
                // Editing a part must audition the edit, even if the user previously chose Before.
                self.player.controls.after.store(true, Ordering::Relaxed);
                self.player.controls.matched.store(false, Ordering::Relaxed);
                self.status = "Live mix · update mix preview before saving or further processing".into();
            }
            Ok(false) => self.error = Some("Live stem playback is unavailable. Update mix preview to prepare audio; processing/export remain available.".into()),
            Err(error) => self.error = Some(format!("Cannot monitor stem controls: {error:#}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn fader_edit_selects_live_b_without_replacing_audio_seeking_or_stopping() {
        let mut studio = super::super::interaction_tests::fixture(2);
        assert!(studio.player.has_live_mix());
        let before = studio.current.as_ref().unwrap().audio.clone();
        studio.player.seek(0.25);
        studio.player.controls.after.store(false, Ordering::Relaxed);
        studio
            .player
            .controls
            .playing
            .store(true, Ordering::Relaxed);
        studio
            .player
            .controls
            .matched
            .store(true, Ordering::Relaxed);
        studio.mix.tracks[0].mute = true;
        studio.monitor_mix_changes();
        assert!(studio.error.is_none(), "{:?}", studio.error);
        assert!(studio.mix_dirty && studio.monitoring_mix);
        assert!(studio.player.controls.after.load(Ordering::Relaxed));
        assert!(studio.player.controls.playing.load(Ordering::Relaxed));
        assert!(!studio.player.controls.matched.load(Ordering::Relaxed));
        assert!(!studio.busy());
        assert_eq!(studio.player.seconds(), 0.25);
        assert!(Arc::ptr_eq(
            &before,
            &studio.current.as_ref().unwrap().audio
        ));
        assert!(
            studio.snapshot().is_err(),
            "stale preview must not be saved as the live mix"
        );
    }
}
