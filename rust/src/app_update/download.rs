//! Stages a verified, inert installer in a fresh directory in the owned cache.
use super::{
    UpdateCandidate, UpdateProgress, VerifiedInstaller, WINDOWS_X86_64, catalog::check_cancel,
    check::host_platform, identity::current, manifest::is_newer,
};
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

pub fn download(
    candidate: &UpdateCandidate,
    cache_root: &Path,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(UpdateProgress),
) -> Result<VerifiedInstaller> {
    download_with(
        candidate,
        cache_root,
        host_platform(),
        cancel,
        progress,
        &crate::downloads::fetch_verified,
    )
}

pub(super) fn download_with<F>(
    candidate: &UpdateCandidate,
    cache_root: &Path,
    platform: &str,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(UpdateProgress),
    fetch: &F,
) -> Result<VerifiedInstaller>
where
    F: Fn(&str, &str, &Path, usize, Arc<AtomicBool>, &dyn Fn(u64)) -> Result<PathBuf> + ?Sized,
{
    check_cancel(&cancel)?;
    ensure!(
        platform == WINDOWS_X86_64,
        "This platform requires a manual update from the official release"
    );
    ensure!(
        is_newer(candidate.identity(), &current())?,
        "This update is already installed"
    );
    ensure!(
        cache_root.is_absolute(),
        "The update cache root must be an absolute path"
    );
    let cache = cache_root.join("app-updates");
    // Reject redirection within our owned cache. A new staging directory also
    // prevents malformed size claims or failures from replacing previous files.
    if cache.symlink_metadata().is_ok() {
        verify_directory(&cache)?;
    }
    std::fs::create_dir_all(&cache).context("Cannot create the application update cache")?;
    verify_directory(&cache)?;
    let staging = tempfile::Builder::new()
        .prefix(&format!("build-{}-", candidate.build_revision()))
        .tempdir_in(&cache)?;
    let destination = staging.path().join(candidate.installer_asset());
    progress(UpdateProgress::Downloading {
        received_bytes: 0,
        total_bytes: candidate.installer_bytes(),
    });
    check_cancel(&cancel)?;
    let path = fetch(
        &candidate.installer_url,
        candidate.installer_sha256(),
        &destination,
        usize::try_from(candidate.installer_bytes())
            .context("Installer byte limit is unsupported on this platform")?,
        cancel.clone(),
        &|bytes| {
            progress(UpdateProgress::Downloading {
                received_bytes: bytes,
                total_bytes: candidate.installer_bytes(),
            })
        },
    )
    .context("Cannot download the verified application installer")?;
    check_cancel(&cancel)?;
    ensure!(
        path == destination,
        "Download returned a path outside the owned staging directory"
    );
    progress(UpdateProgress::Verifying);
    verify_file(&path, candidate.installer_bytes())?;
    check_cancel(&cancel)?;
    let verified = VerifiedInstaller {
        path,
        identity: candidate.identity().clone(),
        platform: candidate.platform().into(),
        installer_sha256: candidate.installer_sha256().into(),
        installer_bytes: candidate.installer_bytes(),
        executable_sha256: candidate.executable_sha256().into(),
        release_url: candidate.release_url().into(),
    };
    let _retained_cache_directory = staging.keep();
    progress(UpdateProgress::Ready);
    Ok(verified)
}

impl VerifiedInstaller {
    /// Call immediately before opening the interactive installer. The download is
    /// inert and a cached file may have changed since the background job ended.
    pub fn verify(&self, cancel: Arc<AtomicBool>) -> Result<()> {
        check_cancel(&cancel)?;
        verify_file(&self.path, self.installer_bytes)?;
        let actual = crate::source_identity::SourceIdentity::read(&self.path, &|| {
            cancel.load(std::sync::atomic::Ordering::Relaxed)
        })?;
        ensure!(
            actual.bytes == self.installer_bytes && actual.sha256 == self.installer_sha256,
            "Cached installer changed; download the update again before installing"
        );
        check_cancel(&cancel)
    }
}

fn verify_file(path: &Path, bytes: u64) -> Result<()> {
    let metadata = path
        .symlink_metadata()
        .context("Cached installer is missing")?;
    ensure!(
        metadata.is_file() && !redirected(&metadata),
        "Cached installer must be an ordinary file"
    );
    ensure!(
        metadata.len() == bytes,
        "Installer byte count differs from the verified manifest"
    );
    Ok(())
}

fn verify_directory(path: &Path) -> Result<()> {
    let metadata = path.symlink_metadata()?;
    ensure!(
        metadata.is_dir() && !redirected(&metadata),
        "Update cache must be an ordinary owned directory"
    );
    Ok(())
}

fn redirected(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}
