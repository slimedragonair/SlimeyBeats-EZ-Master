use super::{
    ToolKind,
    cache::{self, Receipt},
};
use crate::{
    app_update::{self, CatalogResult},
    optional_tools::archive::{self, FixedEntry, Limits},
    source_identity::SourceIdentity,
};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const LIMITS: Limits = Limits {
    max_archive_bytes: 192 * 1024 * 1024,
    max_entries: 256,
    max_entry_bytes: 128 * 1024 * 1024,
    max_total_bytes: 384 * 1024 * 1024,
};
pub(super) fn ensure(
    root: &Path,
    kind: ToolKind,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(f32, &str),
) -> Result<PathBuf> {
    ensure!(
        cfg!(all(windows, target_arch = "x86_64")),
        "This release supplies helper downloads for Windows x64. Build the Linux helper from source."
    );
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    let base = cache::owned(root, kind)?;
    archive::validate_cache_path(&base.join("setup.lock"))?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(base.join("setup.lock"))?;
    let waiting = Instant::now();
    loop {
        ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
        match lock.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) => {
                ensure!(
                    waiting.elapsed() < Duration::from_secs(900),
                    "Another helper setup is still active; please retry later"
                );
                progress(0.01, "Another window is preparing this helper; waiting…");
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(error).context("Cannot lock the helper cache"),
        }
    }
    if let Ok(Some(path)) = cache::ready(root, kind) {
        progress(1., "Local helper is ready");
        return Ok(path);
    }
    progress(0.02, "Checking the official helper download…");
    let catalog = match app_update::fetch_catalog(cancel.clone(), &|_| {})? {
        CatalogResult::Available(catalog) => catalog,
        CatalogResult::Manual { reason, .. } => anyhow::bail!("{reason}"),
    };
    let spec = catalog.manifest().components.get(kind.key()).context(
        "This release has no recovery bundle for this helper; use the full Studio installer",
    )?;
    ensure!(
        spec.protocol == kind.protocol() && spec.files.contains_key(kind.executable()),
        "Helper protocol or executable is incompatible"
    );
    ensure!(
        spec.bytes <= LIMITS.max_archive_bytes && spec.files.len() <= 32,
        "Helper bundle exceeds its limits"
    );
    ensure!(
        spec.files
            .keys()
            .all(|name| !name.contains('/') && !name.contains('\\')),
        "Helper bundle requires flat fixed filenames"
    );
    let asset = catalog
        .asset(&spec.asset)
        .context("Helper release asset is missing")?;
    let stage = tempfile::Builder::new()
        .prefix("bundle-")
        .tempdir_in(&base)?;
    let archive_path = stage.path().join("download.zip");
    crate::downloads::fetch_verified(
        asset.download_url(),
        &spec.sha256,
        &archive_path,
        spec.bytes as usize,
        cancel.clone(),
        &|bytes| {
            progress(
                0.05 + 0.65 * (bytes as f32 / spec.bytes.max(1) as f32),
                &format!(
                    "Downloading {} · {} MiB",
                    kind.display_name(),
                    bytes / 1048576
                ),
            )
        },
    )?;
    ensure!(
        archive_path.metadata()?.len() == spec.bytes,
        "Helper bundle size differs from manifest"
    );
    let files = stage.path().join("files");
    fs::create_dir(&files)?;
    let entries: Vec<_> = spec
        .files
        .keys()
        .map(|name| FixedEntry {
            archive_path: name.as_str(),
            file_name: name.as_str(),
        })
        .collect();
    archive::extract_zip_fixed(
        &archive_path,
        &spec.sha256,
        &files,
        &entries,
        LIMITS,
        &cancel,
    )?;
    for (name, expected) in &spec.files {
        ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
        ensure!(
            SourceIdentity::read(&files.join(name), &|| cancel.load(Ordering::Relaxed))?.sha256
                == *expected,
            "Helper file failed verification: {name}"
        );
    }
    progress(0.85, "Testing the verified helper…");
    let executable = files.join(kind.executable());
    let mut command =
        crate::audio::external_command(executable.to_str().context("Helper path is not Unicode")?);
    command.arg("--version").current_dir(&files);
    let mut output = Vec::new();
    crate::native_process::read_stdout(
        command,
        65536,
        Duration::from_secs(10),
        cancel.clone(),
        |bytes| {
            output.extend_from_slice(bytes);
            Ok(())
        },
    )?;
    verify_version(kind, &output)?;
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    let directory = format!(
        "ready-{}",
        stage.path().file_name().unwrap().to_string_lossy()
    );
    fs::rename(&files, base.join(&directory))?;
    let receipt = Receipt {
        owner: "SlimeyBeats-EZ-Master".into(),
        tool: kind.key().into(),
        protocol: kind.protocol(),
        directory,
        files: spec.files.clone(),
    };
    let mut file = tempfile::NamedTempFile::new_in(&base)?;
    serde_json::to_writer(&mut file, &receipt)?;
    file.flush()?;
    file.as_file().sync_all()?;
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    file.persist(base.join("current.json"))?;
    progress(1., "Verified helper is ready");
    cache::ready(root, kind)?.context("Verified helper could not be found after setup")
}

pub(super) fn verify_version(kind: ToolKind, bytes: &[u8]) -> Result<()> {
    let text = std::str::from_utf8(bytes)?.trim();
    let expected = format!("{} ", kind.executable().trim_end_matches(".exe"));
    let version = text
        .strip_prefix(&expected)
        .context("Downloaded helper reported a different program identity")?;
    semver::Version::parse(version).context("Downloaded helper reported an invalid version")?;
    Ok(())
}
