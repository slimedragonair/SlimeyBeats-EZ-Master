//! Only fixed, pinned application dependencies are passed to this helper.
use crate::{native_process, source_identity::SourceIdentity};
use anyhow::{Result, ensure};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

pub(super) fn fetch(
    url: &str,
    sha256: &str,
    destination: &Path,
    max_bytes: usize,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(u64),
) -> Result<PathBuf> {
    if destination.is_file() && SourceIdentity::read(destination, &|| false)?.sha256 == sha256 {
        return Ok(destination.into());
    }
    let parent = destination.parent().expect("owned download directory");
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    let mut command =
        crate::audio::external_command(if cfg!(windows) { "curl.exe" } else { "curl" });
    command
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "20",
            "--max-time",
            "900",
        ])
        .arg(url);
    let mut count = 0_u64;
    let mut last_report = Instant::now();
    let mut last_bytes = 0_u64;
    native_process::read_stdout(
        command,
        max_bytes,
        Duration::from_secs(910),
        cancel.clone(),
        |bytes| {
            file.write_all(bytes)?;
            count += bytes.len() as u64;
            if last_report.elapsed() >= Duration::from_millis(250)
                || count - last_bytes >= 8 * 1024 * 1024
            {
                progress(count);
                last_report = Instant::now();
                last_bytes = count;
            }
            Ok(())
        },
    )?;
    progress(count);
    file.flush()?;
    file.as_file().sync_all()?;
    let actual = SourceIdentity::read(file.path(), &|| {
        cancel.load(std::sync::atomic::Ordering::Relaxed)
    })?;
    ensure!(
        actual.sha256 == sha256,
        "Downloaded dependency failed its SHA-256 check; previous installation preserved"
    );
    file.persist(destination)?;
    Ok(destination.into())
}
