use anyhow::{Context, Result};
use serde::Serialize;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) const SAVE_DELAY: Duration = Duration::from_millis(750);

/// Debounced atomic writes. A failed load blocks automatic replacement until an explicit save.
pub(super) struct PreferenceStore {
    path: PathBuf,
    saved: Vec<u8>,
    pending: Option<(Vec<u8>, Instant)>,
    failed: Option<Vec<u8>>,
    automatic: bool,
}

impl PreferenceStore {
    pub fn new(path: PathBuf, value: &impl Serialize, automatic: bool) -> Self {
        Self {
            path,
            saved: serde_json::to_vec_pretty(value).expect("valid preferences"),
            pending: None,
            failed: None,
            automatic,
        }
    }

    pub fn update(
        &mut self,
        value: &impl Serialize,
        now: Instant,
        immediate: bool,
        explicit: bool,
    ) -> Result<()> {
        if !self.automatic && !explicit {
            return Ok(());
        }
        let bytes = serde_json::to_vec_pretty(value)?;
        if !explicit && bytes == self.saved {
            self.pending = None;
            return Ok(());
        }
        if !explicit && !immediate && self.failed.as_ref() == Some(&bytes) {
            return Ok(());
        }
        if self
            .pending
            .as_ref()
            .is_none_or(|(value, _)| value != &bytes)
        {
            self.pending = Some((bytes.clone(), now + SAVE_DELAY));
        }
        if !explicit && !immediate && self.pending.as_ref().is_some_and(|(_, due)| now < *due) {
            return Ok(());
        }
        let result = self.write(&bytes);
        self.pending = None;
        match result {
            Ok(()) => {
                self.saved = bytes;
                self.failed = None;
                self.automatic = true;
                Ok(())
            }
            Err(error) => {
                self.failed = Some(bytes);
                Err(error)
            }
        }
    }

    fn write(&self, bytes: &[u8]) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("Preference file needs a parent folder")?;
        std::fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        use std::io::Write;
        temp.write_all(bytes)?;
        temp.as_file().sync_all()?;
        temp.persist(&self.path)
            .map_err(|error| error.error)
            .with_context(|| format!("Save preferences {}", self.path.display()))?;
        Ok(())
    }

    pub fn due_in(&self, now: Instant) -> Option<Duration> {
        self.pending
            .as_ref()
            .map(|(_, due)| due.saturating_duration_since(now))
    }

    pub fn cancel_pending(&mut self) {
        self.pending = None;
    }
}

#[cfg(test)]
#[path = "preference_store_tests.rs"]
mod tests;
