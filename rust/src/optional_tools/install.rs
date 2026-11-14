use super::{
    Paths, Runtime,
    archive::{self, FixedEntry},
    cache::{self, Receipt},
    pins::{self, Pin},
    probes, readiness, smoke, tar,
};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

impl Paths {
    /// Explicit setup entrypoint. No startup/lookup/MCP caller implicitly invokes it.
    /// Prepares this private root even when the global API has selected system tools.
    pub fn ensure(&self, progress: &dyn Fn(f32, &str), cancel: Arc<AtomicBool>) -> Result<Runtime> {
        let pin = pins::current().context("Automatic FFmpeg setup supports Windows x64 and glibc Linux x64 only; install system FFmpeg on this platform")?;
        self.ensure_with(
            pin,
            progress,
            cancel,
            crate::downloads::fetch_verified,
            smoke::verify,
        )
    }

    pub(super) fn ensure_with(
        &self,
        pin: &Pin<'_>,
        progress: &dyn Fn(f32, &str),
        cancel: Arc<AtomicBool>,
        fetch: impl Fn(&str, &str, &Path, usize, Arc<AtomicBool>, &dyn Fn(u64)) -> Result<PathBuf>,
        smoke: impl Fn(&Runtime, &Path, &Arc<AtomicBool>) -> Result<()>,
    ) -> Result<Runtime> {
        archive::check_cancel(&cancel)?;
        cache::prepare(self)?;
        progress(0.01, "Preparing optional codec setup");
        let _lock = cache::lock(self, &cancel, &|| {
            progress(0.01, "Another window is preparing codecs; waiting...")
        })?;
        match cache::ready(self, pin, &cancel) {
            Ok(Some(runtime)) => {
                progress(0.02, "Checking private FFmpeg FLAC and MP3 support");
                match readiness::check(&runtime, &self.root, &cancel, &smoke) {
                    Ok(()) => {
                        progress(1.0, "Private FFmpeg and ffprobe are ready");
                        return Ok(runtime);
                    }
                    Err(error) => progress(
                        0.02,
                        &format!("Preparing a replacement for unusable FFmpeg: {error}"),
                    ),
                }
            }
            Ok(None) => {}
            Err(error) => progress(
                0.01,
                &format!("Checking a replacement for the damaged optional FFmpeg cache: {error}"),
            ),
        }
        archive::check_cancel(&cancel)?;
        let stage = tempfile::Builder::new()
            .prefix(".ffmpeg-stage-")
            .tempdir_in(&self.root)?;
        let downloaded = stage.path().join("archive");
        progress(0.02, "Downloading the pinned optional FFmpeg bundle");
        let path = fetch(
            pin.url,
            pin.sha256,
            &downloaded,
            pin.max_download,
            cancel.clone(),
            &|bytes| {
                progress(
                    0.02 + 0.60 * (bytes as f32 / pin.max_download as f32).min(1.0),
                    &format!("Downloading FFmpeg: {} MiB", bytes / (1024 * 1024)),
                )
            },
        )?;
        ensure!(
            path == downloaded,
            "Downloader returned an unexpected staging path"
        );
        archive::check_cancel(&cancel)?;
        let payload = stage.path().join("payload");
        fs::create_dir(&payload)?;
        progress(0.65, "Verifying and extracting the fixed FFmpeg files");
        extract(pin, &downloaded, &payload, stage.path(), &cancel)?;
        cache::verify_files(&payload, pin, &cancel)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for name in &pin.files[..2] {
                fs::set_permissions(payload.join(name), fs::Permissions::from_mode(0o755))?;
            }
        }
        let runtime = cache::runtime(&payload, pin);
        progress(
            0.85,
            "Checking FLAC and MP3 encode, ffprobe metadata, and decode",
        );
        smoke(&runtime, stage.path(), &cancel)?;
        archive::check_cancel(&cancel)?;
        write_notices(&payload, pin)?;
        let token = stage
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .context("Invalid staging directory")?
            .trim_start_matches(".ffmpeg-stage-");
        let installation = format!("runtime-{}-{token}", &pin.sha256[..16]);
        let installs = self.root.join("installs");
        super::paths::safe_path(&installs)?;
        fs::create_dir_all(&installs)?;
        let destination = installs.join(&installation);
        super::paths::safe_path(&destination)?;
        ensure!(
            !destination.try_exists()?,
            "Installation destination already exists"
        );
        let receipt = Receipt::new(pin, installation);
        let mut receipt_file = File::create(payload.join("receipt.json"))?;
        serde_json::to_writer(&mut receipt_file, &receipt)?;
        receipt_file.sync_all()?;
        drop(receipt_file);
        archive::check_cancel(&cancel)?;
        fs::rename(&payload, &destination)?;
        let runtime = cache::runtime(&destination, pin);
        if let Err(error) = readiness::remember(&runtime, true)
            .and_then(|()| cache::publish_receipt(self, &receipt, &cancel))
        {
            fs::remove_dir_all(&destination)
                .context("Cannot remove this failed, unpublished FFmpeg staging installation")?;
            return Err(error);
        }
        // Publication is the commit point. Cancellation after it cannot undo readiness.
        progress(1.0, "Private FFmpeg and ffprobe are ready");
        Ok(runtime)
    }
}

fn extract(
    pin: &Pin<'_>,
    archive_path: &Path,
    payload: &Path,
    stage: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<()> {
    let members = pin.members();
    let entries: Vec<_> = members
        .iter()
        .zip(pin.files)
        .map(|(path, file_name)| FixedEntry {
            archive_path: path,
            file_name,
        })
        .collect();
    if pin.zip {
        return archive::extract_zip_fixed(
            archive_path,
            pin.sha256,
            payload,
            &entries,
            archive::FFMPEG_LIMITS,
            cancel,
        );
    }
    // Verification precedes starting any archive process. XZ receives no output paths.
    archive::verified_file(archive_path, pin.sha256, pin.max_download as u64, cancel)?;
    let xz = probes::system_in(std::env::var_os("PATH").as_deref(), "xz", &|_| true)
        .context("Private FFmpeg setup on Linux requires the system xz utility; no global packages were installed")?;
    let unpacked = stage.join("verified.tar");
    let mut file = File::create(&unpacked)?;
    let mut command = probes::command(xz);
    command
        .args([
            "--decompress",
            "--stdout",
            "--memlimit-decompress=256MiB",
            "--",
        ])
        .arg(archive_path);
    crate::native_process::read_stdout(
        command,
        archive::FFMPEG_LIMITS.max_total_bytes as usize,
        Duration::from_secs(120),
        cancel.clone(),
        |bytes| {
            file.write_all(bytes)?;
            Ok(())
        },
    )?;
    file.sync_all()?;
    drop(file);
    tar::extract(
        &mut File::open(unpacked)?,
        payload,
        &entries,
        archive::FFMPEG_LIMITS,
        cancel,
    )
}

fn write_notices(directory: &Path, pin: &Pin<'_>) -> Result<()> {
    let notices = format!(
        "Optional FFmpeg runtime, downloaded separately from SlimeyBeats.\n\
        Provider: {}\nRelease: {}\nAsset: {}\nArchive SHA-256: {}\n\
        Official provider listing: https://ffmpeg.org/download.html\n\
        FFmpeg source: {}\nBuild recipes and dependency source/license references: {}/tree/{}\n\
        This provider's LGPL variant excludes GPL-only/nonfree dependency variants.\n\
        Bundled LICENSE.txt is the provider's GNU LGPL version 3 notice.\n\
        LGPL v3 incorporates GPL v3: https://www.gnu.org/licenses/gpl-3.0.html\n\
        Dependency licenses and source locations: {}/tree/{}/scripts.d\n\
        Provider Linux requirements: glibc >= 2.28, kernel >= 4.18.\n\
        This is a pinned dated release, not latest. GitHub release immutable=false;\n\
        compiled SHA-256 pins enforce content identity. Monthly retention: two years.\n\
        No global PATH changes, system installation, admin access, or plugin downloads.\n",
        pins::PROVIDER,
        pins::RELEASE,
        pin.url,
        pin.sha256,
        pins::FFMPEG_SOURCE,
        pins::PROVIDER,
        pins::BUILD_COMMIT,
        pins::PROVIDER,
        pins::BUILD_COMMIT
    );
    let mut file = File::create(directory.join("NOTICE.txt"))?;
    file.write_all(notices.as_bytes())?;
    file.sync_all()?;
    Ok(())
}
