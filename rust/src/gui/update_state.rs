//! Background update jobs and small persistent preferences; never processes audio.
use super::preference_store::PreferenceStore;
use crate::app_update::{self, CheckResult, UpdateCandidate, UpdateProgress, VerifiedInstaller};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct UpdatePreferences {
    pub automatic: bool,
    pub last_check: u64,
}
impl Default for UpdatePreferences {
    fn default() -> Self {
        Self {
            automatic: true,
            last_check: 0,
        }
    }
}
enum Event {
    Progress(UpdateProgress),
    ToolProgress(f32, String),
    Checked(Result<CheckResult, String>),
    Downloaded(Result<VerifiedInstaller, String>),
    Launched(Result<(), String>),
    Codecs(Result<(), String>),
}
pub(super) struct UpdateState {
    pub preferences: UpdatePreferences,
    pub available: Option<UpdateCandidate>,
    pub downloaded: Option<VerifiedInstaller>,
    pub progress: Option<UpdateProgress>,
    pub tool_fraction: Option<f32>,
    pub message: String,
    pub error: Option<String>,
    pub save_error: Option<String>,
    pub confirm_install: bool,
    pub close_after_launch: bool,
    pub opening_setup: bool,
    pub reveal_progress: bool,
    pub reveal_confirmation: bool,
    receiver: Option<mpsc::Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    store: Option<PreferenceStore>,
    startup_checked: bool,
}
impl UpdateState {
    pub fn for_tests() -> Self {
        Self {
            preferences: UpdatePreferences {
                automatic: false,
                last_check: 0,
            },
            available: None,
            downloaded: None,
            progress: None,
            tool_fraction: None,
            message: "Check for a newer application build.".into(),
            error: None,
            save_error: None,
            confirm_install: false,
            close_after_launch: false,
            opening_setup: false,
            reveal_progress: false,
            reveal_confirmation: false,
            receiver: None,
            cancel: Arc::new(AtomicBool::new(false)),
            store: None,
            startup_checked: true,
        }
    }
    pub fn load(path: PathBuf) -> Self {
        let mut state = Self::for_tests();
        let loaded = if path.exists() {
            (|| -> anyhow::Result<UpdatePreferences> {
                let mut bytes = Vec::new();
                std::fs::File::open(&path)?
                    .take(16385)
                    .read_to_end(&mut bytes)?;
                anyhow::ensure!(
                    bytes.len() <= 16384,
                    "Update preferences exceed their size limit"
                );
                Ok(serde_json::from_slice(&bytes)?)
            })()
        } else {
            Ok(UpdatePreferences::default())
        };
        let valid = loaded.is_ok();
        match loaded {
            Ok(value) => state.preferences = value,
            Err(error) => {
                state.save_error = Some(format!(
                    "Update preferences could not be read; the original file is preserved. {error:#}"
                ))
            }
        }
        state.store = Some(PreferenceStore::new(path, &state.preferences, valid));
        state.startup_checked = !valid;
        state
    }
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }
    pub fn persist(&mut self, explicit: bool) {
        if let Some(store) = &mut self.store {
            match store.update(&self.preferences, Instant::now(), true, explicit) {
                Ok(()) => {
                    if explicit {
                        self.save_error = None;
                    }
                }
                Err(error) => {
                    self.save_error = Some(format!("Cannot remember update preferences: {error:#}"))
                }
            }
        }
    }
    pub fn startup(&mut self, ctx: &egui::Context, idle: bool) {
        if self.startup_checked || !idle {
            return;
        }
        self.startup_checked = true;
        let now = unix_seconds();
        if self.preferences.automatic && now.saturating_sub(self.preferences.last_check) >= 86400 {
            self.check(ctx);
        }
    }
    fn begin(&mut self) -> mpsc::Sender<Event> {
        self.cancel = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.error = None;
        self.progress = None;
        self.tool_fraction = None;
        self.confirm_install = false;
        self.opening_setup = false;
        self.reveal_progress = true;
        self.reveal_confirmation = false;
        sender
    }
    pub fn check(&mut self, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        self.preferences.last_check = unix_seconds();
        self.persist(false);
        let sender = self.begin();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        self.message = "Checking the official release…".into();
        std::thread::spawn(move || {
            let progress = |p| {
                let _ = sender.send(Event::Progress(p));
                ctx.request_repaint();
            };
            let result = app_update::check(cancel, &progress).map_err(|e| format!("{e:#}"));
            let _ = sender.send(Event::Checked(result));
            ctx.request_repaint();
        });
    }
    pub fn download(&mut self, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let Some(candidate) = self.available.clone() else {
            return;
        };
        let root = match cache_root() {
            Ok(root) => root,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let sender = self.begin();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        self.message = "Downloading the update; your audio stays open.".into();
        std::thread::spawn(move || {
            let progress = |p| {
                let _ = sender.send(Event::Progress(p));
                ctx.request_repaint();
            };
            let result = app_update::download(&candidate, &root, cancel, &progress)
                .map_err(|e| format!("{e:#}"));
            let _ = sender.send(Event::Downloaded(result));
            ctx.request_repaint();
        });
    }
    pub fn launch(&mut self, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let Some(installer) = self.downloaded.clone() else {
            return;
        };
        let sender = self.begin();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        self.opening_setup = true;
        self.message = "Verifying the installer before opening Setup…".into();
        std::thread::spawn(move || {
            let result =
                super::update_launch::open(&installer, cancel).map_err(|e| format!("{e:#}"));
            let _ = sender.send(Event::Launched(result));
            ctx.request_repaint();
        });
    }
    pub fn prepare_codecs(&mut self, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let sender = self.begin();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        self.message = "Preparing local audio codecs…".into();
        std::thread::spawn(move || {
            let result = crate::optional_tools::ensure(
                &|fraction, message| {
                    let _ = sender.send(Event::ToolProgress(fraction, message.into()));
                    ctx.request_repaint();
                },
                cancel,
            )
            .map(|_| ())
            .map_err(|e| format!("{e:#}"));
            let _ = sender.send(Event::Codecs(result));
            ctx.request_repaint();
        });
    }
    pub fn cancel(&mut self) {
        if self.opening_setup {
            return;
        }
        self.cancel.store(true, Ordering::Relaxed);
        self.message = "Cancelling; your audio is unchanged.".into();
    }
    pub fn poll(&mut self) {
        loop {
            let event = match &self.receiver {
                Some(receiver) => receiver.try_recv(),
                None => return,
            };
            match event {
                Ok(Event::Progress(value)) => self.progress = Some(value),
                Ok(Event::ToolProgress(fraction, message)) => {
                    self.tool_fraction = Some(fraction);
                    self.message = message;
                }
                Ok(event) => {
                    self.receiver = None;
                    self.progress = None;
                    self.tool_fraction = None;
                    self.opening_setup = false;
                    if self.cancel.load(Ordering::Relaxed) {
                        self.message = "Cancelled. Your audio is unchanged.".into();
                        return;
                    }
                    let error = match event {
                        Event::Checked(Ok(CheckResult::Available(value))) => {
                            if !self.downloaded.as_ref().is_some_and(|ready| {
                                ready.identity == *value.identity()
                                    && ready.installer_sha256 == value.installer_sha256()
                                    && ready.installer_bytes == value.installer_bytes()
                            }) {
                                self.downloaded = None;
                            }
                            self.message = "A newer build is available.".into();
                            self.available = Some(value);
                            None
                        }
                        Event::Checked(Ok(CheckResult::UpToDate { .. })) => {
                            self.message = "Your installed build is up to date.".into();
                            self.available = None;
                            self.downloaded = None;
                            None
                        }
                        Event::Checked(Ok(CheckResult::Manual { reason, .. })) => {
                            self.message = reason;
                            self.available = None;
                            self.downloaded = None;
                            None
                        }
                        Event::Downloaded(Ok(value)) => {
                            self.message =
                                "Update verified and ready. Install when your work is saved."
                                    .into();
                            self.downloaded = Some(value);
                            None
                        }
                        Event::Launched(Ok(())) => {
                            self.close_after_launch = true;
                            None
                        }
                        Event::Codecs(Ok(())) => {
                            self.message =
                                "FFmpeg and ffprobe are ready. No restart is needed.".into();
                            None
                        }
                        Event::Launched(Err(e)) => {
                            self.downloaded = None;
                            Some(e)
                        }
                        Event::Checked(Err(e))
                        | Event::Downloaded(Err(e))
                        | Event::Codecs(Err(e)) => Some(e),
                        Event::Progress(_) | Event::ToolProgress(..) => unreachable!(),
                    };
                    self.error = error;
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.progress = None;
                    self.tool_fraction = None;
                    self.opening_setup = false;
                    self.error = Some("The update worker stopped. Please try again.".into());
                    return;
                }
            }
        }
    }
}
impl Drop for UpdateState {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
#[cfg(test)]
#[path = "update_state_tests.rs"]
mod tests;
pub(super) fn cache_root() -> anyhow::Result<PathBuf> {
    crate::paths::cache_root()
        .ok_or_else(|| anyhow::anyhow!("No absolute per-user cache folder is available"))
}
