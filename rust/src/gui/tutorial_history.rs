//! A one-way choice marker keeps stale app windows from re-enabling automatic tutorials.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

fn marker(preferences: &Path) -> PathBuf {
    preferences.with_file_name("tutorial-dismissed.marker")
}

pub(super) fn dismissed(preferences: &Path) -> bool {
    marker(preferences).is_file()
}

pub(super) fn remember(preferences: &Path) -> Result<()> {
    let path = marker(preferences);
    if path.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().context("Missing preference folder")?)?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file.sync_all().context("Remember tutorial choice"),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && path.is_file() => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("Remember tutorial choice {}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remembering_a_choice_is_idempotent_and_does_not_rewrite_preferences() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("interface-preferences.json");
        std::fs::write(&path, "preserved").unwrap();
        remember(&path).unwrap();
        remember(&path).unwrap();
        assert!(dismissed(&path));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "preserved");
    }
}
