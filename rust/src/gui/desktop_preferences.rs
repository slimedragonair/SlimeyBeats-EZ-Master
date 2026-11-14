use super::{app::MasterCraftApp, preference_store::PreferenceStore, studio::Studio};
use crate::config::{FORMATS, Settings};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Instant};

pub(super) fn load_mastering_settings(
    path: &std::path::Path,
    initial: Option<Settings>,
) -> (Settings, Option<String>) {
    let (saved, error) = if path.exists() {
        match Settings::load(path) {
            Ok(settings) => (settings, None),
            Err(error) => (
                Settings::default(),
                Some(format!(
                    "Settings could not be loaded; file preserved. Use Save settings to replace it: {error:#}"
                )),
            ),
        }
    } else {
        (Settings::default(), None)
    };
    // Session settings are a launch override, not permission to overwrite a damaged user file.
    (initial.unwrap_or(saved), error)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct DesktopPreferences {
    studio_output_dir: Option<PathBuf>,
    studio_export_format: String,
    volume: f32,
    match_levels: bool,
    studio_volume: f32,
    studio_match_levels: bool,
    two_stems: bool,
}

impl Default for DesktopPreferences {
    fn default() -> Self {
        Self {
            studio_output_dir: None,
            studio_export_format: "wav24".into(),
            volume: 0.5,
            match_levels: true,
            studio_volume: 0.5,
            studio_match_levels: false,
            two_stems: true,
        }
    }
}

impl DesktopPreferences {
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let value: Self = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            FORMATS.contains(&self.studio_export_format.as_str()),
            "Invalid Studio export format"
        );
        ensure!(
            (0.0..=1.0).contains(&self.volume) && (0.0..=1.0).contains(&self.studio_volume),
            "Invalid listening volume"
        );
        ensure!(
            self.studio_output_dir
                .as_ref()
                .is_none_or(|p| !p.as_os_str().is_empty()),
            "Empty Studio export folder"
        );
        Ok(())
    }
    pub fn apply(&self, studio: &mut Studio) {
        if let Some(path) = &self.studio_output_dir {
            studio.output_dir = path.clone();
        }
        studio.export_format = self.studio_export_format.clone();
        studio.volume = self.studio_volume;
        studio.match_levels = self.studio_match_levels;
        studio.two_stems = self.two_stems;
    }
    pub fn resolve_output(&mut self, fallback: &std::path::Path) {
        if self.studio_output_dir.is_none() {
            self.studio_output_dir = Some(fallback.to_path_buf());
        }
    }
    fn capture(app: &MasterCraftApp) -> Self {
        Self {
            studio_output_dir: Some(app.studio.output_dir.clone()),
            studio_export_format: app.studio.export_format.clone(),
            volume: app.volume,
            match_levels: app.match_levels,
            studio_volume: app.studio.volume,
            studio_match_levels: app.studio.match_levels,
            two_stems: app.studio.two_stems,
        }
    }
    pub fn volume(&self) -> f32 {
        self.volume
    }
    pub fn match_levels(&self) -> bool {
        self.match_levels
    }
}

pub(super) struct PreferencePersistence {
    settings: PreferenceStore,
    desktop: PreferenceStore,
}

impl PreferencePersistence {
    pub fn new(
        settings_path: PathBuf,
        settings: &Settings,
        settings_valid: bool,
        desktop_path: PathBuf,
        desktop: &DesktopPreferences,
        desktop_valid: bool,
    ) -> Self {
        Self {
            settings: PreferenceStore::new(settings_path, settings, settings_valid),
            desktop: PreferenceStore::new(desktop_path, desktop, desktop_valid),
        }
    }
}

impl MasterCraftApp {
    pub(super) fn persist_preferences(&mut self, immediate: bool, explicit: bool) {
        let desktop = DesktopPreferences::capture(self);
        let now = Instant::now();
        let Some(persistence) = &mut self.preferences else {
            return;
        };
        let mut errors = vec![];
        // A temporarily incomplete editor value must not replace the last valid settings.
        match self.settings.validate() {
            Ok(()) => {
                if let Err(error) =
                    persistence
                        .settings
                        .update(&self.settings, now, immediate, explicit)
                {
                    errors.push(format!("Save settings: {error:#}"));
                }
            }
            Err(error) => {
                persistence.settings.cancel_pending();
                if explicit {
                    errors.push(format!("Save settings: {error:#}"));
                }
            }
        }
        if let Err(error) = desktop.validate().and_then(|()| {
            persistence
                .desktop
                .update(&desktop, now, immediate, explicit)
        }) {
            errors.push(format!("Save desktop preferences: {error:#}"));
        }
        if !errors.is_empty() {
            let message = errors.join("\n");
            eprintln!("{message}");
            self.error = Some(message);
        } else if explicit {
            self.status = "Settings and desktop preferences saved".into();
        }
    }

    pub(super) fn autosave_preferences(&mut self, ctx: &eframe::egui::Context) {
        self.persist_preferences(false, false);
        if let Some(persistence) = &self.preferences {
            let now = Instant::now();
            for delay in [
                persistence.settings.due_in(now),
                persistence.desktop.due_in(now),
            ]
            .into_iter()
            .flatten()
            {
                ctx.request_repaint_after(delay);
            }
        }
    }
}

#[cfg(test)]
#[path = "desktop_preferences_tests.rs"]
mod tests;
