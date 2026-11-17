//! Connect safe snapshot capture/restore to the native editing lifecycle.
use super::{app::MasterCraftApp, recovery_capture, recovery_key, recovery_state::PROBE_INTERVAL};
use crate::recovery_store::CheckpointRef;
use eframe::egui;
use std::{sync::atomic::Ordering, time::Instant};

impl MasterCraftApp {
    pub(super) fn has_recovery_work(&self) -> bool {
        !self.tracks.is_empty()
            || self.studio.source.is_some()
            || self.studio.has_recovery_arrangement()
    }
    pub(super) fn save_recovery_now(&mut self, ctx: &egui::Context) {
        if self.recovery.busy() {
            return;
        }
        let key = recovery_key::key(self);
        match key
            .as_ref()
            .map_err(|e| anyhow::anyhow!("{e:#}"))
            .and_then(|key| Ok((*key, recovery_capture::capture(self)?)))
        {
            Ok((key, capture)) => {
                self.recovery.failed_key = None;
                self.recovery.save(key, capture, ctx);
            }
            Err(error) => {
                self.recovery.failed_key = key.ok();
                self.recovery.error = Some(format!("Cannot prepare a recovery copy: {error:#}"));
                self.recovery.message =
                    "Your current session is kept. Save a project or export manually.".into();
            }
        }
    }
    pub(super) fn request_recovery_restore(&mut self, key: CheckpointRef, ctx: &egui::Context) {
        if self.recovery.busy() || self.busy || self.studio.busy() {
            return;
        }
        self.recovery.confirm_restore = false;
        self.player.stop();
        self.studio.player.stop();
        if self.has_recovery_work() {
            // Back up the exact current draft before replacing it, including
            // live controls that have not been materialized for export.
            self.save_recovery_now(ctx);
            if self.recovery.busy() {
                self.recovery.restoring = true;
                self.recovery.block_focus = true;
                self.recovery.restore_after_save = Some(key);
            }
        } else {
            self.recovery.load_selected(key, ctx);
        }
    }
    pub(super) fn tick_recovery(&mut self, ctx: &egui::Context) {
        if let Some(prepared) = self.recovery.poll() {
            prepared.adopt(self);
            self.recovery.last_probe = None;
            self.recovery.observed_key = None;
            self.recovery.refresh(ctx);
        }
        let permitted = !self.capture_session || self.recovery.diagnostic;
        if !permitted {
            return;
        }
        if !self.recovery.startup_done && !self.recovery.busy() {
            self.recovery.refresh(ctx);
        }
        if self.recovery.clear_current_requested && !self.recovery.busy() {
            self.recovery.clear_current(ctx);
        }
        if !self.recovery.busy()
            && let Some(key) = self.recovery.restore_after_save.take()
        {
            self.recovery.load_selected(key, ctx);
        }
        if self.recovery.diagnostic_restore
            && self.recovery.startup_done
            && !self.recovery.busy()
            && !self.recovery.restore_completed
        {
            self.recovery.diagnostic_restore = false;
            if let Some(key) = self
                .recovery
                .listing
                .checkpoints
                .first()
                .map(|i| i.checkpoint.clone())
            {
                self.request_recovery_restore(key, ctx);
            } else {
                self.recovery.error =
                    Some("The diagnostic recovery folder contains no available snapshot.".into());
            }
        }
        self.recovery.offer_pending(
            !self.has_recovery_work()
                && !self.busy
                && !self.studio.busy()
                && !self.capture_session
                && !self.experience.has_modal(),
        );
        if self.recovery.offer {
            self.experience.suppress_automatic_offer();
        }
        if self.recovery.busy() {
            ctx.request_repaint_after(PROBE_INTERVAL);
        }
        let close = ctx.input(|input| input.viewport().close_requested()) && !self.capture_session;
        if close && self.recovery.restoring && !self.recovery.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.recovery.closing {
                self.recovery.block_focus = true;
            }
            self.recovery.closing = true;
            self.recovery.closing_for_restore = true;
        }
        if self.recovery.restoring {
            return;
        }
        let clearing = self.recovery.clear_current_requested || self.recovery.clearing_current;
        if close
            && !self.recovery.allow_close
            && (clearing || self.recovery.preferences.enabled && self.has_recovery_work())
        {
            let protected = !clearing
                && recovery_key::key(self)
                    .ok()
                    .is_some_and(|key| self.recovery.saved_key == Some(key))
                && !self.recovery.busy();
            if !protected {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if !self.recovery.closing {
                    self.recovery.block_focus = true;
                }
                self.recovery.closing = true;
                self.recovery.closing_for_clear = clearing;
                self.player.stop();
                self.studio.player.stop();
            }
        }
        if self.recovery.closing {
            if self.recovery.closing_for_restore && !self.recovery.busy() {
                if self.recovery.error.is_none() && !self.recovery.preferences.enabled {
                    self.recovery.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                self.recovery.closing_for_restore = false;
            }
            if self.recovery.closing_for_clear {
                if !self.recovery.busy() && !clearing && self.recovery.error.is_none() {
                    self.recovery.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                ctx.request_repaint_after(PROBE_INTERVAL);
                return;
            }
            if !self.recovery.busy() {
                let key = recovery_key::key(self).ok();
                if key.is_some() && key == self.recovery.saved_key {
                    self.recovery.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else if self.recovery.error.is_none() {
                    self.save_recovery_now(ctx);
                }
            }
            ctx.request_repaint_after(PROBE_INTERVAL);
            return;
        }
        if !self.recovery.preferences.enabled || !self.has_recovery_work() {
            return;
        }
        let now = Instant::now();
        if let Some(previous) = self.recovery.last_probe
            && now.duration_since(previous) < PROBE_INTERVAL
        {
            ctx.request_repaint_after(PROBE_INTERVAL - now.duration_since(previous));
            return;
        }
        self.recovery.last_probe = Some(now);
        match recovery_key::key(self) {
            Ok(key) => {
                self.recovery.observe(key, now);
                if self.recovery.dirty_since.is_some() {
                    ctx.request_repaint_after(PROBE_INTERVAL);
                }
                if self.recovery.saved_key != Some(key)
                    && self.recovery.failed_key != Some(key)
                    && self.recovery.due(now)
                    && !self.recovery.busy()
                {
                    self.save_recovery_now(ctx);
                }
            }
            Err(error) => {
                self.recovery.error = Some(format!(
                    "Recovery controls could not be captured: {error:#}"
                ))
            }
        }
    }
    pub(super) fn finish_recovery(&mut self) {
        let clean = !self.recovery.busy()
            && (!self.has_recovery_work()
                || recovery_key::key(self)
                    .ok()
                    .is_some_and(|key| self.recovery.saved_key == Some(key)));
        self.recovery.shutdown(clean);
    }
    pub(super) fn pause_for_recovery(&self) {
        self.player.controls.playing.store(false, Ordering::Relaxed);
        self.studio
            .player
            .controls
            .playing
            .store(false, Ordering::Relaxed);
    }
}
