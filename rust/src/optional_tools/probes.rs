use super::{Runtime, archive, paths::safe_path, readiness, smoke};
use anyhow::{Result, ensure};
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, OnceLock, atomic::AtomicBool},
    time::{Duration, SystemTime},
};

#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    bytes: u64,
    modified: Option<SystemTime>,
}
type Checks = HashMap<(PathBuf, String), Stamp>;
static CHECKS: OnceLock<Mutex<Checks>> = OnceLock::new();
type Selection = Option<(Runtime, Option<OsString>)>;
static SELECTED_SYSTEM: OnceLock<Mutex<Selection>> = OnceLock::new();

pub(super) fn program(name: &str) -> Option<&'static str> {
    match name {
        "ffmpeg" => Some("ffmpeg"),
        "ffprobe" => Some("ffprobe"),
        "ffmpeg.exe" if cfg!(windows) => Some("ffmpeg"),
        "ffprobe.exe" if cfg!(windows) => Some("ffprobe"),
        _ => None,
    }
}

pub(super) fn command(path: impl AsRef<OsStr>) -> Command {
    let command = Command::new(path);
    #[cfg(windows)]
    let mut command = command;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW, including console helpers.
    }
    command
}

pub(super) fn collect(command: Command, max: usize, cancel: Arc<AtomicBool>) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    crate::native_process::read_stdout(command, max, Duration::from_secs(10), cancel, |bytes| {
        result.extend_from_slice(bytes);
        Ok(())
    })?;
    Ok(result)
}

fn checked(
    path: &Path,
    tag: &str,
    cancel: &AtomicBool,
    validate: impl FnOnce() -> Result<()>,
) -> Result<()> {
    archive::check_cancel(cancel)?;
    safe_path(path)?;
    let metadata = fs::metadata(path)?;
    ensure!(metadata.is_file(), "Tool is not an ordinary file");
    let stamp = Stamp {
        bytes: metadata.len(),
        modified: metadata.modified().ok(),
    };
    let key = (path.to_path_buf(), tag.to_owned());
    let checks = CHECKS.get_or_init(|| Mutex::new(HashMap::new()));
    if stamp.modified.is_some() && checks.lock().is_ok_and(|c| c.get(&key) == Some(&stamp)) {
        return Ok(());
    }
    validate()?;
    archive::check_cancel(cancel)?;
    if let Ok(mut checks) = checks.lock() {
        if checks.len() >= 64 {
            checks.clear();
        }
        checks.insert(key, stamp);
    }
    Ok(())
}

pub(super) fn verify(path: &Path, sha: &str, cancel: &AtomicBool) -> Result<()> {
    checked(path, sha, cancel, || {
        archive::verified_file(path, sha, archive::FFMPEG_LIMITS.max_entry_bytes, cancel)?;
        Ok(())
    })
}

pub(super) fn version(path: &Path, name: &str, cancel: &Arc<AtomicBool>) -> Result<()> {
    checked(path, name, cancel, || {
        let mut command = command(path);
        command.arg("-version");
        let output = collect(command, 128 * 1024, cancel.clone())?;
        ensure!(
            String::from_utf8_lossy(&output).starts_with(&format!("{name} version ")),
            "Unexpected {name} version response"
        );
        Ok(())
    })
}

pub(super) fn system(program_name: &str) -> Option<PathBuf> {
    let name = program(program_name)?;
    let selection = SELECTED_SYSTEM
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()?
        .clone()?;
    let runtime = prepared_candidate(
        selection.0,
        selection.1.as_deref(),
        std::env::var_os("PATH").as_deref(),
    )?;
    Some(if name == "ffmpeg" {
        runtime.ffmpeg
    } else {
        runtime.ffprobe
    })
}

pub(super) fn prepared_candidate(
    runtime: Runtime,
    selected_path: Option<&OsStr>,
    current_path: Option<&OsStr>,
) -> Option<Runtime> {
    (selected_path == current_path && readiness::state(&runtime) == Some(true)).then_some(runtime)
}

pub(super) fn system_in(
    path: Option<&OsStr>,
    name: &str,
    probe: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let executable = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    };
    std::env::split_paths(path?)
        .filter(|p| p.is_absolute())
        .take(64)
        .filter_map(|p| p.join(&executable).canonicalize().ok())
        .find(|p| p.is_file() && probe(p))
}

pub(super) fn system_runtime(cancel: &Arc<AtomicBool>) -> Result<Option<Runtime>> {
    archive::check_cancel(cancel)?;
    let resolve = |name| {
        system_in(std::env::var_os("PATH").as_deref(), name, &|p| {
            version(p, name, cancel).is_ok()
        })
    };
    let result = resolve("ffmpeg")
        .zip(resolve("ffprobe"))
        .map(|(ffmpeg, ffprobe)| Runtime {
            ffmpeg,
            ffprobe,
            private: false,
        });
    let result = if let Some(runtime) = result {
        let directory = tempfile::tempdir()?;
        readiness::check(&runtime, directory.path(), cancel, &smoke::verify)
            .ok()
            .map(|()| runtime)
    } else {
        None
    };
    archive::check_cancel(cancel)?;
    *SELECTED_SYSTEM
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| anyhow::anyhow!("Codec selection unavailable"))? =
        result.clone().map(|r| (r, std::env::var_os("PATH")));
    Ok(result)
}
