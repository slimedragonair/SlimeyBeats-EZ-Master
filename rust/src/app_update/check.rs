use super::{
    CatalogResult, CheckResult, MANUAL_RELEASE_URL, ReleaseCatalog, UpdateCandidate,
    UpdateProgress, WINDOWS_X86_64,
    catalog::{check_cancel, fetch_catalog_with},
    identity::{BuildIdentity, current},
    manifest::is_newer,
};
use anyhow::{Context, Result};
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

/// Checks the running platform without changing installed files or preferences.
/// Linux/source builds receive a manual release route; no Linux installer exists.
pub fn check(cancel: Arc<AtomicBool>, progress: &dyn Fn(UpdateProgress)) -> Result<CheckResult> {
    check_with(
        &current(),
        host_platform(),
        cancel,
        progress,
        &crate::downloads::read_metadata,
    )
}

pub(super) fn host_platform() -> &'static str {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        WINDOWS_X86_64
    } else {
        "manual"
    }
}

pub(super) fn check_with<F>(
    installed: &BuildIdentity,
    platform: &str,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(UpdateProgress),
    read: &F,
) -> Result<CheckResult>
where
    F: Fn(&str, usize, Duration, Arc<AtomicBool>) -> Result<Vec<u8>> + ?Sized,
{
    check_cancel(&cancel)?;
    if platform != WINDOWS_X86_64 {
        return Ok(CheckResult::Manual {
            release_url: MANUAL_RELEASE_URL.into(),
            reason: "Automatic installer downloads support Windows x86_64 only. Linux and other source builds must use the official release and manual build/install instructions.".into(),
        });
    }
    match fetch_catalog_with(cancel.clone(), progress, read)? {
        CatalogResult::Available(catalog) => {
            check_cancel(&cancel)?;
            match candidate_from_catalog(&catalog, installed)? {
                Some(candidate) => Ok(CheckResult::Available(candidate)),
                None => Ok(CheckResult::UpToDate {
                    latest: catalog.manifest().identity(),
                }),
            }
        }
        CatalogResult::Manual {
            release_url,
            reason,
        } => Ok(CheckResult::Manual {
            release_url,
            reason,
        }),
    }
}

fn candidate_from_catalog(
    catalog: &ReleaseCatalog,
    installed: &BuildIdentity,
) -> Result<Option<UpdateCandidate>> {
    let manifest = catalog.manifest();
    if !is_newer(&manifest.identity(), installed)? {
        return Ok(None);
    }
    let installer = manifest.platforms.windows_x86_64.clone();
    let asset = catalog
        .asset(&installer.installer_asset)
        .context("Installer asset disappeared from the validated catalog")?;
    Ok(Some(UpdateCandidate {
        identity: manifest.identity(),
        notes: manifest.notes.clone(),
        release_url: catalog.release_url().into(),
        installer_url: asset.download_url().into(),
        installer,
    }))
}
