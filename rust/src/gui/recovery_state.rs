//! Recovery scheduling and job state; disk work belongs exclusively to recovery_worker.
use super::{
    recovery_capture::Capture,
    recovery_preferences::{Persistence, Preferences},
    recovery_restore::Prepared,
    recovery_worker::{self, Command, Outcome, Work, Worker},
};
use crate::recovery_store::{CheckpointInfo, CheckpointRef, Listing};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub(super) const PROBE_INTERVAL: Duration = Duration::from_millis(500);
const SAVE_DELAY: Duration = Duration::from_millis(1500);
const MAX_DIRTY_DELAY: Duration = Duration::from_secs(10);

pub(super) struct RecoveryState {
    pub preferences: Preferences,
    pub root: Option<PathBuf>,
    pub listing: Listing,
    pub selected: Option<CheckpointRef>,
    pub inspected: Option<serde_json::Value>,
    pub last_saved: Option<CheckpointInfo>,
    pub saved_key: Option<u64>,
    pub message: String,
    pub error: Option<String>,
    pub preference_error: Option<String>,
    pub offer: bool,
    pub offer_focus: bool,
    pub block_focus: bool,
    pub reveal_history: bool,
    pub confirm_restore: bool,
    pub confirm_delete: bool,
    pub requested_restore: Option<CheckpointRef>,
    pub restore_after_save: Option<CheckpointRef>,
    pub restoring: bool,
    pub closing: bool,
    pub closing_for_clear: bool,
    pub closing_for_restore: bool,
    pub clearing_current: bool,
    pub allow_close: bool,
    pub restore_completed: bool,
    pub diagnostic: bool,
    pub diagnostic_restore: bool,
    pub capture_wait: bool,
    pub startup_done: bool,
    pub observed_key: Option<u64>,
    pub dirty_since: Option<Instant>,
    pub changed_at: Option<Instant>,
    pub last_probe: Option<Instant>,
    pub failed_key: Option<u64>,
    active_save_key: Option<u64>,
    pub confirm_clear_current: bool,
    pub clear_current_requested: bool,
    persistence: Option<Persistence>,
    worker: Option<Worker>,
    operation: u64,
    active: Option<u64>,
    cancel: Arc<AtomicBool>,
    ack_after_save: Vec<String>,
    startup_offer_pending: bool,
}
impl RecoveryState {
    pub fn for_tests() -> Self {
        Self {
            preferences: Preferences {
                enabled: false,
                offer_on_startup: false,
            },
            root: None,
            listing: Listing::default(),
            selected: None,
            inspected: None,
            last_saved: None,
            saved_key: None,
            message: "Autosave is off.".into(),
            error: None,
            preference_error: None,
            offer: false,
            offer_focus: false,
            block_focus: false,
            reveal_history: false,
            confirm_restore: false,
            confirm_delete: false,
            requested_restore: None,
            restore_after_save: None,
            restoring: false,
            closing: false,
            closing_for_clear: false,
            closing_for_restore: false,
            clearing_current: false,
            allow_close: false,
            restore_completed: false,
            diagnostic: false,
            diagnostic_restore: false,
            capture_wait: false,
            startup_done: true,
            observed_key: None,
            dirty_since: None,
            changed_at: None,
            last_probe: None,
            failed_key: None,
            active_save_key: None,
            confirm_clear_current: false,
            clear_current_requested: false,
            persistence: None,
            worker: None,
            operation: 0,
            active: None,
            cancel: Arc::new(AtomicBool::new(false)),
            ack_after_save: vec![],
            startup_offer_pending: false,
        }
    }
    pub fn load(root: Option<PathBuf>, preferences: PathBuf) -> Self {
        let (prefs, persistence, error) = Persistence::load(preferences);
        let mut state = Self::for_tests();
        state.preferences = prefs;
        state.persistence = Some(persistence);
        state.preference_error = error;
        state.root = root;
        state.startup_done = false;
        state.message = "Recovery history has not been checked yet.".into();
        state
    }
    pub fn diagnostic(root: PathBuf) -> Self {
        let mut state = Self::for_tests();
        state.root = Some(root);
        state.preferences = Preferences::default();
        state.startup_done = false;
        state.diagnostic = true;
        state
    }
    pub fn busy(&self) -> bool {
        self.active.is_some()
    }
    pub fn persist(&mut self, explicit: bool) {
        if let Some(p) = &mut self.persistence {
            match p.save(&self.preferences, explicit) {
                Ok(()) => {
                    if explicit {
                        self.preference_error = None;
                    }
                }
                Err(e) => {
                    self.preference_error = Some(format!("Cannot remember recovery choices: {e:#}"))
                }
            }
        }
    }
    fn send(&mut self, work: Work, ctx: &egui::Context) -> bool {
        if self.busy() {
            return false;
        }
        if self.worker.is_none() {
            let Some(root) = self.root.clone() else {
                self.error = Some(
                    "No private recovery folder is available. Save projects and exports manually."
                        .into(),
                );
                self.startup_done = true;
                return false;
            };
            match recovery_worker::start(root, ctx.clone()) {
                Ok(w) => self.worker = Some(w),
                Err(e) => {
                    self.error = Some(format!("Cannot start local recovery: {e}"));
                    self.startup_done = true;
                    return false;
                }
            }
        }
        self.active_save_key = match &work {
            Work::Save(key, _) => Some(*key),
            _ => None,
        };
        self.operation += 1;
        self.active = Some(self.operation);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.error = None;
        if self
            .worker
            .as_ref()
            .unwrap()
            .sender
            .send(Command::Run(self.operation, work, self.cancel.clone()))
            .is_err()
        {
            self.active = None;
            self.worker = None;
            self.error =
                Some("The recovery worker stopped. Refresh history or retry Save now.".into());
            return false;
        }
        true
    }
    pub fn refresh(&mut self, ctx: &egui::Context) {
        if self.send(Work::History, ctx) {
            self.message = "Checking local recovery history…".into();
        }
    }
    pub fn inspect(&mut self, ctx: &egui::Context) {
        if let Some(key) = self.selected.clone()
            && self.send(Work::Inspect(key), ctx)
        {
            self.message = "Verifying recovery details…".into();
        }
    }
    pub fn save(&mut self, key: u64, capture: Capture, ctx: &egui::Context) -> bool {
        if self.send(Work::Save(key, capture), ctx) {
            self.message = "Saving a local recovery copy…".into();
            true
        } else {
            self.failed_key = Some(key);
            false
        }
    }
    pub fn load_selected(&mut self, key: CheckpointRef, ctx: &egui::Context) {
        if self.send(Work::Restore(key.clone()), ctx) {
            self.requested_restore = Some(key);
            if !self.restoring {
                self.block_focus = true;
            }
            self.restoring = true;
            self.message = "Verifying and loading recovery audio…".into();
        }
    }
    pub fn delete_selected(&mut self, ctx: &egui::Context) {
        if let Some(key) = self.selected.clone()
            && self.send(Work::Delete(key.run_id().into()), ctx)
        {
            self.message = "Removing the selected inactive recovery session…".into();
            self.inspected = None;
        }
    }
    pub fn request_clear_current(&mut self) {
        self.preferences.enabled = false;
        self.persist(false);
        if self.busy() {
            self.cancel();
        }
        self.clear_current_requested = true;
        self.confirm_clear_current = false;
    }
    pub fn clear_current(&mut self, ctx: &egui::Context) {
        let id = self
            .last_saved
            .as_ref()
            .map(|i| i.checkpoint.run_id().to_owned());
        if self.send(Work::ClearCurrent(id), ctx) {
            self.clear_current_requested = false;
            self.clearing_current = true;
            // Once deletion is requested, an old point must not remain labelled
            // as verified protection if cancellation interrupts cleanup later.
            self.last_saved = None;
            self.saved_key = None;
            self.message = "Clearing this window's local recovery copies…".into();
        }
    }
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.restore_after_save = None;
        self.restoring = false;
        self.requested_restore = None;
        self.message = "Cancelling recovery work; the current editing session is kept.".into();
    }
    pub fn due(&self, now: Instant) -> bool {
        self.dirty_since
            .is_some_and(|t| now.duration_since(t) >= MAX_DIRTY_DELAY)
            || self
                .changed_at
                .is_some_and(|t| now.duration_since(t) >= SAVE_DELAY)
    }
    pub fn observe(&mut self, key: u64, now: Instant) {
        if self.observed_key != Some(key) {
            self.observed_key = Some(key);
            self.changed_at = Some(now);
            if self.dirty_since.is_none() {
                self.dirty_since = Some(now);
            }
        }
        if self.saved_key == Some(key) || self.failed_key == Some(key) {
            self.dirty_since = None;
            self.changed_at = None;
        }
    }
    pub fn poll(&mut self) -> Option<Prepared> {
        loop {
            let received = self.worker.as_ref()?.receiver.try_recv();
            match received {
                Ok(event) if self.active == Some(event.id) => {
                    self.active = None;
                    if self.cancel.load(Ordering::Relaxed)
                        && !matches!(
                            &event.result,
                            Ok(Outcome::Saved(..))
                                | Ok(Outcome::Deleted(..))
                                | Ok(Outcome::CurrentCleared(..))
                        )
                    {
                        self.startup_done = true;
                        if self.clearing_current {
                            self.error=Some("Recovery removal was cancelled. Some copies may remain; retry clearing when ready.".into());
                            self.clearing_current = false;
                        }
                        self.failed_key = self.active_save_key.take();
                        self.restoring = false;
                        self.restore_after_save = None;
                        self.message =
                            "Recovery work cancelled. The current session is unchanged.".into();
                        continue;
                    }
                    match event.result {
                        Ok(Outcome::Saved(key, info)) => {
                            self.failed_key = None;
                            self.saved_key = Some(key);
                            self.message = "Recovery copy saved locally.".into();
                            self.error = info.maintenance_warning.clone();
                            self.last_saved = Some(info);
                            let acks = std::mem::take(&mut self.ack_after_save);
                            for id in acks {
                                self.acknowledge(&id);
                            }
                        }
                        Ok(Outcome::History(history)) => {
                            self.listing = history;
                            self.startup_offer_pending = !self.startup_done;
                            self.startup_done = true;
                            if self.selected.as_ref().is_none_or(|key| {
                                !self
                                    .listing
                                    .checkpoints
                                    .iter()
                                    .any(|i| &i.checkpoint == key)
                            }) {
                                self.selected = self
                                    .listing
                                    .checkpoints
                                    .first()
                                    .map(|i| i.checkpoint.clone());
                                self.inspected = None;
                            }
                            self.message = format!(
                                "{} recovery point(s) from earlier, inactive sessions.",
                                self.listing.checkpoints.len()
                            );
                        }
                        Ok(Outcome::Inspected(key, payload)) => {
                            if self.selected.as_ref() == Some(&key) {
                                self.inspected = Some(payload);
                                self.message="Recovery details verified. Audio is verified when you restore.".into();
                            }
                        }
                        Ok(Outcome::Restored(prepared)) => {
                            self.restoring = false;
                            self.restore_completed = true;
                            self.offer = false;
                            self.confirm_restore = false;
                            self.last_saved = None;
                            self.saved_key = None;
                            if let Some(key) = self.requested_restore.take() {
                                self.ack_after_save.push(key.run_id().into());
                            }
                            self.message = "Session recovered. Playback is paused.".into();
                            return Some(*prepared);
                        }
                        Ok(Outcome::Deleted(id)) => {
                            self.acknowledge(&id);
                            self.listing
                                .checkpoints
                                .retain(|i| i.checkpoint.run_id() != id);
                            self.selected = None;
                            self.message="Inactive recovery session removed. Your projects and exports were not changed.".into();
                        }
                        Ok(Outcome::CurrentCleared(id)) => {
                            self.clearing_current = false;
                            if let Some(id) = id {
                                self.acknowledge(&id);
                                self.listing
                                    .checkpoints
                                    .retain(|i| i.checkpoint.run_id() != id);
                            }
                            self.last_saved = None;
                            self.saved_key = None;
                            self.failed_key = None;
                            self.message="Autosave is off and this window's recovery copies were cleared. Editing, projects and exports are unchanged.".into();
                        }
                        Err(error) => {
                            self.clearing_current = false;
                            self.failed_key = self.active_save_key.take();
                            self.startup_done = true;
                            self.restoring = false;
                            self.restore_after_save = None;
                            self.error = Some(error);
                            self.message="Recovery could not complete. The current editing session is kept; inspect or refresh history before relying on stored copies.".into();
                        }
                    }
                    self.active_save_key = None;
                }
                Ok(_) => {}
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.active = None;
                    self.worker = None;
                    self.restoring = false;
                    self.clearing_current = false;
                    self.restore_after_save = None;
                    self.error=Some("The recovery worker stopped. Your editing session is unchanged; retry or save a project manually.".into());
                    return None;
                }
            }
        }
    }
    pub fn offer_pending(&mut self, eligible: bool) {
        if !self.startup_offer_pending || !eligible {
            return;
        }
        self.startup_offer_pending = false;
        self.offer = self.preferences.enabled
            && self.preferences.offer_on_startup
            && self.listing.checkpoints.iter().any(|i| {
                i.interrupted
                    && !self
                        .persistence
                        .as_ref()
                        .is_some_and(|p| p.acknowledged(i.checkpoint.run_id()))
            });
        self.offer_focus = self.offer;
    }
    fn acknowledge(&mut self, id: &str) {
        if let Some(p) = &self.persistence
            && let Err(error) = p.acknowledge(id)
        {
            self.preference_error =
                Some(format!("Could not remember this recovery offer: {error:#}"));
        }
    }
    pub fn keep_for_later(&mut self) {
        let ids: Vec<_> = self
            .listing
            .checkpoints
            .iter()
            .filter(|i| i.interrupted)
            .map(|i| i.checkpoint.run_id().to_owned())
            .collect();
        for id in ids {
            self.acknowledge(&id);
        }
        self.offer = false;
    }
    pub fn shutdown(&mut self, clean: bool) {
        recovery_worker::stop(self.worker.take(), clean, &self.cancel);
        self.active = None;
    }
}
impl Drop for RecoveryState {
    fn drop(&mut self) {
        self.shutdown(false);
    }
}
#[cfg(test)]
#[path = "recovery_state_tests.rs"]
mod tests;
