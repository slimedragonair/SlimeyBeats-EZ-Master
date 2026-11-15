use anyhow::{Context, Result, bail, ensure};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path},
    time::{Duration, Instant},
};

pub(super) fn cancelled(cancel: &dyn Fn() -> bool) -> Result<()> {
    ensure!(!cancel(), "Recovery operation cancelled");
    Ok(())
}
pub(super) fn safe_path(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute(),
        "Recovery storage requires an absolute private path"
    );
    ensure!(
        !path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir)),
        "Recovery path contains traversal"
    );
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) => {
                ensure!(
                    !meta.file_type().is_symlink(),
                    "Recovery path contains a symbolic link"
                );
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        meta.file_attributes() & 0x400 == 0,
                        "Recovery path contains a reparse point"
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error).context("Inspect recovery path"),
        }
    }
    Ok(())
}
pub(super) fn directory(path: &Path) -> Result<()> {
    safe_path(path)?;
    ensure!(
        fs::symlink_metadata(path)?.is_dir(),
        "Expected a recovery directory"
    );
    Ok(())
}
pub(super) fn private_root(path: &Path) -> Result<()> {
    directory(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .context("Cannot restrict recovery storage to the current user")?;
    }
    Ok(())
}
pub(super) fn regular(path: &Path) -> Result<u64> {
    safe_path(path)?;
    let meta = fs::symlink_metadata(path)?;
    ensure!(meta.is_file(), "Expected a regular recovery file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(meta.nlink() == 1, "Recovery file has hard links");
    }
    #[cfg(windows)]
    {
        use std::{ffi::c_void, os::windows::io::AsRawHandle};
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(handle: *mut c_void, info: *mut [u32; 13]) -> i32;
        }
        let file = File::open(path)?;
        let mut info = [0_u32; 13];
        // BY_HANDLE_FILE_INFORMATION is thirteen DWORDs; index ten is nNumberOfLinks.
        ensure!(
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0,
            "Cannot inspect recovery file links: {}",
            std::io::Error::last_os_error()
        );
        ensure!(info[10] == 1, "Recovery file has hard links");
    }
    Ok(meta.len())
}
pub(super) fn read_json<T: DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    ensure!(
        regular(path)? <= limit,
        "Recovery metadata exceeds its size limit"
    );
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Recovery metadata grew past its limit"
    );
    serde_json::from_slice(&bytes).context("Invalid recovery metadata")
}
pub(super) fn json_bytes<T: Serialize>(value: &T, limit: u64) -> Result<Vec<u8>> {
    // Bounded serialization prevents an accidental huge payload allocation first.
    let mut writer = LimitedWriter {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut writer, value)
        .context("Recovery metadata exceeds its limit or cannot be serialized")?;
    Ok(writer.bytes)
}
struct LimitedWriter {
    bytes: Vec<u8>,
    limit: u64,
}
impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len() as u64 + bytes.len() as u64 > self.limit {
            return Err(std::io::Error::other("metadata limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn create_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = json_bytes(value, super::MAX_METADATA_BYTES)?;
    create_bytes(path, &bytes)
}
pub(super) fn create_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    safe_path(path)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    #[cfg(test)]
    if let Some(after) = FAIL_NEXT_WRITE.with(|cell| cell.take()) {
        file.write_all(&bytes[..after.min(bytes.len())])?;
        return Err(std::io::Error::from(std::io::ErrorKind::StorageFull))
            .context("Injected disk-full checkpoint write");
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
#[cfg(test)]
thread_local! { pub(super) static FAIL_NEXT_WRITE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) }; }
pub(super) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
pub(super) fn try_lock(path: &Path) -> Result<Option<File>> {
    ensure!(
        regular(path)? == 0,
        "Recovery lock contains unexpected material"
    );
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(error) => Err(error).context("Cannot lock recovery storage"),
    }
}
pub(super) fn lock(path: &Path, cancel: &dyn Fn() -> bool) -> Result<File> {
    let start = Instant::now();
    loop {
        cancelled(cancel)?;
        if let Some(file) = try_lock(path)? {
            return Ok(file);
        }
        ensure!(
            start.elapsed() < Duration::from_secs(30),
            "Another recovery operation is busy; retry later"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}
pub(super) fn entries(path: &Path, max: usize, cancel: &dyn Fn() -> bool) -> Result<Vec<String>> {
    directory(path)?;
    let mut names = Vec::new();
    for entry in fs::read_dir(path)? {
        cancelled(cancel)?;
        ensure!(
            names.len() < max,
            "Recovery directory exceeds its entry limit"
        );
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Non-Unicode recovery filename"))?;
        ensure!(!name.contains(['/', '\\']), "Invalid recovery filename");
        names.push(name);
    }
    names.sort();
    Ok(names)
}
pub(super) fn require_names(
    path: &Path,
    allowed: &[&str],
    cancel: &dyn Fn() -> bool,
) -> Result<()> {
    for name in entries(path, allowed.len() + 1, cancel)? {
        if !allowed.contains(&name.as_str()) {
            bail!("Unknown material preserved in recovery storage: {name}");
        }
    }
    Ok(())
}
pub(super) fn now_ms() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}
