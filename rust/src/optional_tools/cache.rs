use super::{Paths, Runtime, archive::check_cancel, paths::safe_path, pins::Pin, probes};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    thread,
    time::{Duration, Instant},
};

const OWNER: &[u8] = b"SlimeyBeats-EZ-Master optional FFmpeg cache v1\n";
const LOCK_WAIT_TIMEOUT: Duration = Duration::from_secs(120);
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    schema: u32,
    owner: String,
    platform: String,
    archive_url: String,
    archive_sha256: String,
    pub installation: String,
    smoke_tested: bool,
    encoders: [String; 2],
}

impl Receipt {
    pub fn new(pin: &Pin<'_>, installation: String) -> Self {
        Self {
            schema: 1,
            owner: "SlimeyBeats-EZ-Master".into(),
            platform: pin.platform.into(),
            archive_url: pin.url.into(),
            archive_sha256: pin.sha256.into(),
            installation,
            smoke_tested: true,
            encoders: super::smoke::REQUIRED_ENCODERS.map(String::from),
        }
    }
    pub fn matches(&self, pin: &Pin<'_>) -> bool {
        self.schema == 1
            && self.owner == "SlimeyBeats-EZ-Master"
            && self.platform == pin.platform
            && self.archive_url == pin.url
            && self.archive_sha256 == pin.sha256
            && self.smoke_tested
            && self
                .encoders
                .iter()
                .map(String::as_str)
                .eq(super::smoke::REQUIRED_ENCODERS)
            && self.installation.starts_with("runtime-")
            && self.installation.len() <= 96
            && self
                .installation
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }
}

fn read_small(path: &Path, max: u64) -> Result<Vec<u8>> {
    safe_path(path)?;
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= max,
        "Invalid optional-tool cache metadata"
    );
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= max,
        "Optional-tool cache metadata grew beyond its budget"
    );
    Ok(bytes)
}

pub(super) fn owned(paths: &Paths) -> Result<()> {
    safe_path(&paths.root)?;
    ensure!(
        read_small(&paths.root.join("owner.txt"), 128)? == OWNER,
        "This optional-tool cache belongs to another application"
    );
    Ok(())
}

pub(super) fn prepare(paths: &Paths) -> Result<()> {
    safe_path(&paths.root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&paths.root)?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(&paths.root)?;
    let marker = paths.root.join("owner.txt");
    if !marker.try_exists()? {
        ensure!(
            fs::read_dir(&paths.root)?.next().is_none(),
            "Choose a dedicated empty optional-tool cache; unrelated files were preserved"
        );
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
        {
            Ok(mut file) => {
                file.write_all(OWNER)?;
                file.sync_all()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    owned(paths)
}

pub(super) fn lock(paths: &Paths, cancel: &AtomicBool, waiting: &dyn Fn()) -> Result<File> {
    let path = paths.root.join("setup.lock");
    safe_path(&path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    let start = Instant::now();
    let mut announced = false;
    loop {
        check_cancel(cancel)?;
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => {
                if !announced {
                    waiting();
                    announced = true;
                }
                check_cancel(cancel)?;
                ensure!(
                    start.elapsed() < LOCK_WAIT_TIMEOUT,
                    "Another FFmpeg setup is still active; retry when it finishes"
                );
                thread::sleep(LOCK_POLL_INTERVAL);
            }
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "Cannot lock the optional-tool cache: {error}"
                ));
            }
        }
    }
}

pub(super) fn ready(
    paths: &Paths,
    pin: &Pin<'_>,
    cancel: &Arc<AtomicBool>,
) -> Result<Option<Runtime>> {
    check_cancel(cancel)?;
    let receipt_path = paths.root.join("ready.json");
    safe_path(&receipt_path)?;
    if !receipt_path.try_exists()? {
        return Ok(None);
    }
    owned(paths)?;
    let receipt: Receipt = serde_json::from_slice(&read_small(&receipt_path, 16 * 1024)?)?;
    ensure!(
        receipt.matches(pin),
        "Optional-tool receipt does not match the pinned bundle"
    );
    let directory = paths.root.join("installs").join(&receipt.installation);
    verify_files(&directory, pin, cancel)?;
    let runtime = runtime(&directory, pin);
    Ok(Some(runtime))
}

pub(super) fn verify_files(directory: &Path, pin: &Pin<'_>, cancel: &AtomicBool) -> Result<()> {
    for (file, sha) in pin.files.iter().zip(pin.hashes) {
        probes::verify(&directory.join(file), sha, cancel)
            .with_context(|| format!("Private FFmpeg file failed verification: {file}"))?;
    }
    Ok(())
}

pub(super) fn runtime(directory: &Path, pin: &Pin<'_>) -> Runtime {
    Runtime {
        ffmpeg: directory.join(pin.files[0]),
        ffprobe: directory.join(pin.files[1]),
        private: true,
    }
}

pub(super) fn publish_receipt(paths: &Paths, receipt: &Receipt, cancel: &AtomicBool) -> Result<()> {
    let destination = paths.root.join("ready.json");
    safe_path(&destination)?;
    let mut staged = tempfile::NamedTempFile::new_in(&paths.root)?;
    serde_json::to_writer(&mut staged, receipt)?;
    staged.flush()?;
    staged.as_file().sync_all()?;
    check_cancel(cancel)?;
    // tempfile persist replaces the pointer atomically, including on Windows.
    staged
        .persist(destination)
        .context("Cannot publish optional-tool receipt; previous receipt preserved")?;
    Ok(())
}
