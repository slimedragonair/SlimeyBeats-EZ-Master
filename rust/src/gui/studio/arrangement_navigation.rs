//! Tracks and legacy tools share one player, but retain separate paused audition state.
use super::state::{Page, Studio};
use anyhow::{Result, ensure};
use eframe::egui;
use std::sync::atomic::Ordering;

impl Studio {
    /// Route native Studio tabs without committing edits or resuming either transport.
    /// Legacy-to-legacy navigation keeps its existing running player untouched.
    pub(in crate::gui) fn activate_page(&mut self, page: Page, ctx: &egui::Context) {
        if (self.page == Page::Tracks) == (page == Page::Tracks) {
            self.page = page;
            return;
        }
        self.player.controls.playing.store(false, Ordering::Relaxed);
        if self.player.has_live_arrangement() {
            self.arrangement.position = self.player.seconds();
            self.arrangement.after = self.player.controls.after.load(Ordering::Relaxed);
        } else {
            self.remember_legacy_playback();
        }
        // A late or absent preparation must leave silence, never the other pane's engine.
        self.player.unload();
        self.page = page;
        if page == Page::Tracks {
            self.player
                .controls
                .after
                .store(self.arrangement.after, Ordering::Relaxed);
            self.player.controls.matched.store(false, Ordering::Relaxed);
            if self
                .arrangement
                .editor
                .document()
                .is_some_and(|document| document.duration_frames() > 0)
            {
                self.prepare_arrangement_player(ctx);
            }
        } else if let Err(error) = self.restore_legacy_playback() {
            self.player.unload();
            self.error = Some(format!(
                "Playback unavailable: {error:#}. Processing/export remain available."
            ));
        }
    }

    /// Imports/preparations can replace a legacy engine even while Tracks is selected.
    pub(super) fn remember_legacy_playback(&mut self) {
        // activate_page already unloaded its outgoing player before async admission.
        // Do not replace the saved position/side with that empty player's defaults.
        if !self.player.has_live_arrangement() && self.player.duration > 0. {
            self.arrangement.legacy_position = self.player.seconds();
            self.arrangement.legacy_after = self.player.controls.after.load(Ordering::Relaxed);
        }
    }

    fn restore_legacy_playback(&mut self) -> Result<()> {
        let Some(revision) = &self.current else {
            return Ok(());
        };
        // load_player is a revision-adoption path: it clears analysis/spectrograms
        // and monitoring state. Tab navigation only restores the playback engine.
        if let Some(bank) = &self.bank {
            self.player.load_stems(
                revision.before.clone(),
                revision.audio.clone(),
                (revision.before_lufs, revision.after_lufs),
                bank.clone(),
                &self.mix,
            )?;
            if self.monitoring_mix {
                ensure!(
                    self.player.set_mix(bank, &self.mix)?,
                    "Cannot restore live stem controls"
                );
            }
        } else {
            self.player.load(
                revision.before.clone(),
                Some(revision.audio.clone()),
                revision.before_lufs,
                revision.after_lufs,
                None,
            )?;
        }
        self.player
            .controls
            .after
            .store(self.arrangement.legacy_after, Ordering::Relaxed);
        self.player
            .controls
            .matched
            .store(self.match_levels && !self.monitoring_mix, Ordering::Relaxed);
        self.player
            .controls
            .volume
            .store(self.volume.to_bits(), Ordering::Relaxed);
        self.player
            .seek(self.arrangement.legacy_position.min(self.player.duration));
        self.player.set_loop(if self.editor.looping {
            self.editor.selection
        } else {
            None
        })?;
        Ok(())
    }
}
