use super::{MAX_REPORT_BYTES, Report};
use anyhow::{Context, Result, ensure};
use std::{
    io::{self, Write},
    path::Path,
};

impl Report {
    /// Publish the exact preview bytes locally. Refuses every existing target,
    /// including symlinks, and never creates a missing parent directory.
    pub fn write_new(&self, destination: &Path) -> Result<()> {
        ensure!(
            destination.is_absolute(),
            "Choose an absolute report destination"
        );
        ensure!(
            self.markdown().len() <= MAX_REPORT_BYTES,
            "Support report exceeds 64 KiB"
        );
        ensure!(
            destination.file_name().is_some(),
            "Choose a report filename"
        );
        let parent = destination
            .parent()
            .context("Choose a report destination folder")?;
        // tempfile uses private user permissions on Unix. The chosen folder's
        // permissions still matter; this is not encrypted storage.
        let mut staged = tempfile::Builder::new()
            .prefix(".slimeybeats-support-")
            .tempfile_in(parent)
            .map_err(|error| io_failure("Could not prepare a local support report file", &error))?;
        staged
            .write_all(self.markdown().as_bytes())
            .map_err(|error| io_failure("Could not write the support report", &error))?;
        staged
            .as_file()
            .sync_all()
            .map_err(|error| io_failure("Could not finish writing the support report", &error))?;
        staged.persist_noclobber(destination).map_err(|error| {
            // Dropping PersistError also removes its still-owned temporary file.
            // Never include the destination or temporary path in the error text.
            io_failure(
                "Could not save support report; choose a new filename",
                &error.error,
            )
        })?;
        Ok(())
    }
}

fn io_failure(message: &str, error: &io::Error) -> anyhow::Error {
    // tempfile can embed a selected path inside an io::Error's payload. Expose
    // typed failure information, never its arbitrary Display/source chain.
    anyhow::anyhow!(
        "{message} ({:?}; OS code {:?})",
        error.kind(),
        error.raw_os_error()
    )
}
