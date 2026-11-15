//! Allocate audio scratch only when needed, on the authorized output volume.
use anyhow::{Context, Result};
use std::{
    collections::hash_map::RandomState,
    hash::BuildHasher,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use tempfile::TempDir;

pub struct SessionCache {
    output: PathBuf,
    directory: OnceLock<TempDir>,
    creation: Mutex<()>,
}

impl SessionCache {
    pub fn new(output: PathBuf) -> Self {
        Self {
            output,
            directory: OnceLock::new(),
            creation: Mutex::new(()),
        }
    }

    pub fn path(&self) -> Result<&Path> {
        if let Some(directory) = self.directory.get() {
            return Ok(directory.path());
        }
        let _creation = self
            .creation
            .lock()
            .map_err(|_| anyhow::anyhow!("Session scratch initialization is unavailable"))?;
        if self.directory.get().is_none() {
            // Keep large previews off the system temp drive and preserve the
            // output-relative preview contract. Failed creation stays retryable.
            let directory = tempfile::Builder::new()
                .prefix(".slimeybeats-mcp-")
                .tempdir_in(&self.output)
                .context("Cannot create temporary audio scratch in the output folder")?;
            self.directory
                .set(directory)
                .expect("Session scratch initialization is serialized");
        }
        Ok(self.directory.get().expect("initialized scratch").path())
    }
}

pub(crate) fn connection_id() -> String {
    // IDs must not allocate a directory: hosts can forcibly terminate even a
    // read-only discovery connection, bypassing TempDir's normal Drop cleanup.
    format!("{:016x}", RandomState::new().hash_one(std::process::id()))
}

#[cfg(test)]
#[path = "session_cache_tests.rs"]
mod tests;
