//! Bounded HTTPS downloads and SHA-256 publication for application-managed components.
use crate::{native_process, source_identity::SourceIdentity};
use anyhow::{Result, ensure};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

pub fn fetch_verified(
    url: &str,
    sha256: &str,
    destination: &Path,
    max_bytes: usize,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(u64),
) -> Result<PathBuf> {
    validate_url(url)?;
    ensure!(
        sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid download SHA-256"
    );
    ensure!(
        !cancel.load(std::sync::atomic::Ordering::Relaxed),
        "Cancelled"
    );
    if destination.is_file()
        && SourceIdentity::read(destination, &|| {
            cancel.load(std::sync::atomic::Ordering::Relaxed)
        })?
        .sha256
            == sha256
    {
        return Ok(destination.into());
    }
    let parent = destination.parent().expect("owned download directory");
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    let mut command =
        crate::audio::external_command(if cfg!(windows) { "curl.exe" } else { "curl" });
    command
        .args([
            "--globoff",
            "--max-redirs",
            "5",
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
        .arg("--")
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
    ensure!(
        !cancel.load(std::sync::atomic::Ordering::Relaxed),
        "Cancelled"
    );
    file.persist(destination)?;
    Ok(destination.into())
}

fn validate_url(url: &str) -> Result<()> {
    ensure!(
        url.starts_with("https://")
            && url.len() <= 8192
            && !url
                .bytes()
                .any(|b| b.is_ascii_whitespace() || b.is_ascii_control()),
        "Downloads require a valid HTTPS URL"
    );
    let authority = url[8..].split('/').next().unwrap_or("");
    ensure!(
        !authority.is_empty() && !authority.contains('@'),
        "Invalid download authority"
    );
    Ok(())
}

pub fn read_metadata(
    url: &str,
    max_bytes: usize,
    timeout: Duration,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    validate_url(url)?;
    ensure!(
        (1..=4 * 1024 * 1024).contains(&max_bytes),
        "Invalid metadata byte limit"
    );
    let mut command =
        crate::audio::external_command(if cfg!(windows) { "curl.exe" } else { "curl" });
    command
        .args([
            "--globoff",
            "--fail",
            "--location",
            "--max-redirs",
            "5",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "15",
            "--max-time",
        ])
        .arg(timeout.as_secs().max(1).to_string())
        .args([
            "--header",
            "Accept: application/vnd.github+json",
            "--user-agent",
            "SlimeyBeats-EZ-Master",
        ])
        .arg("--")
        .arg(url);
    let mut result = Vec::new();
    native_process::read_stdout(command, max_bytes, timeout, cancel, |bytes| {
        result.extend_from_slice(bytes);
        Ok(())
    })?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_transport_or_digest_never_touches_previous_download() {
        let root = tempfile::tempdir().unwrap();
        let previous = root.path().join("setup.exe");
        std::fs::write(&previous, b"keep the existing file").unwrap();
        for (url, sha) in [
            ("http://example.test/tool", "a".repeat(64)),
            ("--output=somewhere", "a".repeat(64)),
            ("https://example.test/\r\nfile", "a".repeat(64)),
            ("https://user@example.test/file", "a".repeat(64)),
            ("https://example.test/file", "bad".into()),
        ] {
            assert!(
                fetch_verified(
                    url,
                    &sha,
                    &previous,
                    100,
                    Arc::new(AtomicBool::new(false)),
                    &|_| {}
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&previous).unwrap(), b"keep the existing file");
        }
    }
    #[test]
    fn already_cancelled_download_preserves_verified_cache() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("cache.zip");
        std::fs::write(&path, b"verified").unwrap();
        let sha = SourceIdentity::read(&path, &|| false).unwrap().sha256;
        assert!(
            fetch_verified(
                "https://example.test/file",
                &sha,
                &path,
                100,
                Arc::new(AtomicBool::new(true)),
                &|_| {}
            )
            .is_err()
        );
        assert_eq!(std::fs::read(path).unwrap(), b"verified");
    }
}
