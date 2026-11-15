//! Recovery choices and one-way offer acknowledgements, separate from audio data.
use super::preference_store::PreferenceStore;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{io::Read, path::PathBuf, time::Instant};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Preferences {
    pub enabled: bool,
    pub offer_on_startup: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            enabled: true,
            offer_on_startup: true,
        }
    }
}
pub(super) struct Persistence {
    store: PreferenceStore,
    acknowledgements: PathBuf,
}
impl Persistence {
    pub fn load(path: PathBuf) -> (Preferences, Self, Option<String>) {
        let loaded = (|| -> Result<Preferences> {
            if !path.exists() {
                return Ok(Preferences::default());
            }
            let mut bytes = Vec::new();
            std::fs::File::open(&path)?
                .take(16385)
                .read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() <= 16384,
                "Recovery preferences exceed their size limit"
            );
            Ok(serde_json::from_slice(&bytes)?)
        })();
        let valid = loaded.is_ok();
        let (prefs, error) = match loaded {
            Ok(p) => (p, None),
            Err(e) => (
                Preferences {
                    enabled: false,
                    offer_on_startup: false,
                },
                Some(format!(
                    "Recovery preferences could not be read; original file preserved. {e:#}"
                )),
            ),
        };
        let acknowledgements = path.with_file_name("recovery-offers-dismissed");
        let store = PreferenceStore::new(path, &prefs, valid);
        (
            prefs,
            Self {
                store,
                acknowledgements,
            },
            error,
        )
    }
    pub fn save(&mut self, prefs: &Preferences, explicit: bool) -> Result<()> {
        self.store.update(prefs, Instant::now(), true, explicit)
    }
    fn acknowledgement(&self, run_id: &str) -> Result<PathBuf> {
        ensure!(
            !run_id.is_empty()
                && run_id.len() <= 128
                && run_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
            "Invalid recovery offer identity"
        );
        let path = self.acknowledgements.join(format!("{run_id}.marker"));
        crate::optional_tools::archive::validate_cache_path(&path)?;
        Ok(path)
    }
    pub fn acknowledged(&self, run_id: &str) -> bool {
        self.acknowledgement(run_id).is_ok_and(|p| p.is_file())
    }
    pub fn acknowledge(&self, run_id: &str) -> Result<()> {
        let path = self.acknowledgement(run_id)?;
        std::fs::create_dir_all(&self.acknowledgements)?;
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(file) => file.sync_all().map_err(Into::into),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
