//! Bounded positive/negative capability cache for a complete FFmpeg/ffprobe pair.
use super::{Runtime, archive, paths::safe_path};
use anyhow::{Result, ensure};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, atomic::AtomicBool},
    time::SystemTime,
};

#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    files: [(PathBuf, u64, SystemTime); 2],
}
struct Check {
    snapshot: Snapshot,
    ready: bool,
}
static CHECKS: OnceLock<Mutex<Vec<Check>>> = OnceLock::new();

fn snapshot(runtime: &Runtime) -> Result<Snapshot> {
    let stamp = |path: &PathBuf| -> Result<_> {
        safe_path(path)?;
        let metadata = fs::metadata(path)?;
        ensure!(metadata.is_file(), "Codec tool is not an ordinary file");
        Ok((path.clone(), metadata.len(), metadata.modified()?))
    };
    Ok(Snapshot {
        files: [stamp(&runtime.ffmpeg)?, stamp(&runtime.ffprobe)?],
    })
}

pub(super) fn state(runtime: &Runtime) -> Option<bool> {
    let snapshot = snapshot(runtime).ok()?;
    CHECKS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .ok()?
        .iter()
        .find(|c| c.snapshot == snapshot)
        .map(|c| c.ready)
}

pub(super) fn remember(runtime: &Runtime, ready: bool) -> Result<()> {
    let snapshot = snapshot(runtime)?;
    let mut checks = CHECKS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .map_err(|_| anyhow::anyhow!("Codec readiness cache unavailable"))?;
    checks.retain(|c| {
        c.snapshot.files[0].0 != snapshot.files[0].0 || c.snapshot.files[1].0 != snapshot.files[1].0
    });
    if checks.len() >= 16 {
        checks.remove(0);
    }
    checks.push(Check { snapshot, ready });
    Ok(())
}

pub(super) fn check(
    runtime: &Runtime,
    directory: &std::path::Path,
    cancel: &Arc<AtomicBool>,
    test: &impl Fn(&Runtime, &std::path::Path, &Arc<AtomicBool>) -> Result<()>,
) -> Result<()> {
    archive::check_cancel(cancel)?;
    if let Some(ready) = state(runtime) {
        ensure!(
            ready,
            "This FFmpeg pair failed the required FLAC/libmp3lame encode, probe and decode checks"
        );
        archive::check_cancel(cancel)?;
        return Ok(());
    }
    let before = snapshot(runtime)?;
    let result = test(runtime, directory, cancel);
    archive::check_cancel(cancel)?;
    ensure!(
        before == snapshot(runtime)?,
        "Codec executables changed during their readiness probes; retry setup"
    );
    remember(runtime, result.is_ok())?;
    result
}
