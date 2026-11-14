use super::{
    guide_content::guide,
    interface_preferences::{InterfacePreferences, TutorialStatus},
    preference_store::PreferenceStore,
};
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum HelpPage {
    Start,
    Terms,
    Studio,
    Assistant,
    Troubleshooting,
    About,
}

pub(super) struct UserExperience {
    pub preferences: InterfacePreferences,
    pub help_page: Option<HelpPage>,
    pub welcome_open: bool,
    pub tour_open: bool,
    pub tour_paused: bool,
    pub request_dialog_focus: bool,
    pub search: String,
    pub save_error: Option<String>,
    pub mcp: super::mcp_setup::McpSetup,
    store: Option<PreferenceStore>,
    offer_allowed: bool,
    storage_blocked: bool,
    return_focus: Option<egui::Id>,
    pending_focus: Option<egui::Id>,
}

impl UserExperience {
    pub fn load(path: PathBuf) -> Self {
        let loaded = if path.exists() {
            Self::read(&path)
        } else {
            Ok(InterfacePreferences::default())
        };
        let (mut preferences, save_error, blocked) = match loaded {
            Ok(value) => (value, None, false),
            Err(error) => (
                InterfacePreferences {
                    tutorial_status: TutorialStatus::Declined,
                    ..Default::default()
                },
                Some(format!(
                    "Interface preferences could not be read. The file is preserved; your audio settings are unaffected. {error}"
                )),
                true,
            ),
        };
        preferences.tutorial_step = preferences.tutorial_step.min(guide().steps.len() - 1);
        if preferences.should_offer() && super::tutorial_history::dismissed(&path) {
            preferences.tutorial_status = TutorialStatus::Declined;
        }
        let store = Some(PreferenceStore::new(path, &preferences, !blocked));
        Self {
            preferences,
            help_page: None,
            welcome_open: false,
            tour_open: false,
            tour_paused: false,
            request_dialog_focus: true,
            search: String::new(),
            save_error,
            mcp: Default::default(),
            store,
            offer_allowed: !blocked,
            storage_blocked: blocked,
            return_focus: None,
            pending_focus: None,
        }
    }

    fn read(path: &Path) -> anyhow::Result<InterfacePreferences> {
        let text = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(text.trim_start_matches('\u{feff}'))?)
    }

    pub fn advanced(&self) -> bool {
        self.preferences.advanced
    }

    pub fn offer_when_idle(&mut self, idle: bool) {
        if self
            .store
            .as_ref()
            .is_some_and(|store| super::tutorial_history::dismissed(store.path()))
        {
            self.offer_allowed = false;
        }
        if idle
            && self.offer_allowed
            && self.preferences.should_offer()
            && self.help_page.is_none()
            && !self.tour_open
        {
            self.welcome_open = true;
            self.request_dialog_focus = true;
            self.offer_allowed = false;
        }
    }

    pub fn suppress_automatic_offer(&mut self) {
        self.offer_allowed = false;
        self.welcome_open = false;
    }

    pub fn open_help(&mut self, page: HelpPage, ctx: &egui::Context) {
        if self.help_page.is_none() {
            self.return_focus = self
                .pending_focus
                .take()
                .or_else(|| ctx.memory(|memory| memory.focused()));
        }
        self.help_page = Some(page);
        self.welcome_open = false;
        self.tour_paused |= self.tour_open;
        self.tour_open = false;
        self.request_dialog_focus = true;
    }

    pub fn close_help(&mut self, _ctx: &egui::Context) {
        self.help_page = None;
        self.mcp.cancel();
        self.pending_focus = self.return_focus.take();
    }

    pub fn restore_pending_focus(&mut self, ctx: &egui::Context) {
        if self.has_modal() || self.pending_focus.is_none() {
            return;
        }
        if ctx.memory(|memory| memory.top_modal_layer().is_none()) {
            if let Some(id) = self.pending_focus.take() {
                ctx.memory_mut(|memory| memory.request_focus(id));
            }
        } else {
            ctx.request_repaint();
        }
    }

    pub fn start_tour(&mut self) {
        self.welcome_open = false;
        self.help_page = None;
        self.tour_open = true;
        self.tour_paused = false;
        self.request_dialog_focus = true;
        if self.preferences.tutorial_status != TutorialStatus::Started {
            self.preferences.tutorial_step = 0;
        }
        self.preferences.tutorial_status = TutorialStatus::Started;
        self.persist(true, false);
    }

    pub fn replay_tour(&mut self) {
        // An explicit replay must not make an already-dismissed tour automatic again.
        self.welcome_open = false;
        self.help_page = None;
        self.tour_open = true;
        self.tour_paused = false;
        self.request_dialog_focus = true;
        self.preferences.tutorial_step = 0;
    }

    pub fn finish_tour(&mut self, completed: bool) {
        self.welcome_open = false;
        self.tour_open = false;
        self.tour_paused = false;
        self.offer_allowed = false;
        self.preferences.tutorial_status = if completed {
            TutorialStatus::Completed
        } else {
            TutorialStatus::Declined
        };
        let marker_error = self
            .store
            .as_ref()
            .and_then(|store| super::tutorial_history::remember(store.path()).err());
        self.persist(true, false);
        if let Some(error) = marker_error {
            self.save_error = Some(format!(
                "The tutorial choice could not be fully stored: {error:#}"
            ));
        }
    }

    pub fn pause_tour(&mut self) {
        self.tour_open = false;
        self.tour_paused = true;
        self.persist(true, false);
    }

    pub fn resume_tour(&mut self) {
        self.tour_paused = false;
        self.tour_open = true;
        self.request_dialog_focus = true;
    }

    pub fn has_modal(&self) -> bool {
        self.welcome_open || self.tour_open || self.help_page.is_some()
    }

    pub fn persist(&mut self, immediate: bool, explicit: bool) {
        if self.storage_blocked && !explicit {
            return;
        }
        if let Some(store) = &mut self.store {
            if !explicit && store.matches_saved(&self.preferences).unwrap_or(false) {
                return;
            }
            if !explicit && store.path().exists() {
                match Self::read(store.path()) {
                    Ok(latest) if self.preferences.should_offer() && !latest.should_offer() => {
                        self.preferences.tutorial_status = latest.tutorial_status
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.storage_blocked = true;
                        store.cancel_pending();
                        self.save_error = Some(format!(
                            "The interface preference file changed and could not be read. It is preserved. Use Help to save the current choices explicitly. {error:#}"
                        ));
                        return;
                    }
                }
            }
            if self.preferences.should_offer() && super::tutorial_history::dismissed(store.path()) {
                self.preferences.tutorial_status = TutorialStatus::Declined;
            }
            if !self.preferences.should_offer()
                && let Err(error) = super::tutorial_history::remember(store.path())
            {
                self.save_error = Some(format!(
                    "The tutorial choice could not be fully stored: {error:#}"
                ));
                return;
            }
            match store.update(&self.preferences, Instant::now(), immediate, explicit) {
                Ok(()) => {
                    if explicit || immediate {
                        self.storage_blocked = false;
                        self.save_error = None;
                    }
                }
                Err(error) => {
                    self.save_error = Some(format!(
                        "This device could not remember the interface choice. It may appear again after a restart. {error:#}"
                    ))
                }
            }
        }
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        self.persist(false, false);
        if let Some(delay) = self
            .store
            .as_ref()
            .and_then(|store| store.due_in(Instant::now()))
        {
            ctx.request_repaint_after(delay);
        }
    }

    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self {
            preferences: Default::default(),
            help_page: None,
            welcome_open: false,
            tour_open: false,
            tour_paused: false,
            request_dialog_focus: false,
            search: String::new(),
            save_error: None,
            mcp: Default::default(),
            store: None,
            offer_allowed: false,
            storage_blocked: false,
            return_focus: None,
            pending_focus: None,
        }
    }
}

#[cfg(test)]
#[path = "user_experience_tests.rs"]
mod tests;
