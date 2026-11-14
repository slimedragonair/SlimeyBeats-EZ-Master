//! Read-only official release catalog shared with optional-component recovery.
use super::{
    MANUAL_RELEASE_URL, UpdateProgress,
    manifest::{MAX_MANIFEST_BYTES, UpdateManifest, sha256_hex},
    manifest_asset,
    release::{MAX_RELEASE_BYTES, RELEASE_API, Release, ReleaseAsset},
};
use anyhow::{Context, Result, ensure};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const METADATA_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub enum CatalogResult {
    Available(ReleaseCatalog),
    Manual { release_url: String, reason: String },
}

/// Only `fetch_catalog` can construct a catalog. No mutable metadata is exposed.
#[derive(Clone, Debug)]
pub struct ReleaseCatalog {
    manifest: Box<UpdateManifest>,
    release_url: String,
    assets: Vec<ReleaseAsset>,
}

impl ReleaseCatalog {
    pub fn manifest(&self) -> &UpdateManifest {
        &self.manifest
    }
    pub fn release_url(&self) -> &str {
        &self.release_url
    }

    /// Exact-name lookup; URLs and IDs were bound to this official release.
    pub fn asset(&self, name: &str) -> Option<&ReleaseAsset> {
        self.assets.iter().find(|asset| asset.name() == name)
    }
}

/// Fetches metadata only. Component consumers must check their supported protocol
/// and extraction allowlist before using the validated, bounded asset locator.
pub fn fetch_catalog(
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(UpdateProgress),
) -> Result<CatalogResult> {
    fetch_catalog_with(cancel, progress, &crate::downloads::read_metadata)
}

pub(super) fn fetch_catalog_with<F>(
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(UpdateProgress),
    read: &F,
) -> Result<CatalogResult>
where
    F: Fn(&str, usize, Duration, Arc<AtomicBool>) -> Result<Vec<u8>> + ?Sized,
{
    check_cancel(&cancel)?;
    progress(UpdateProgress::CheckingRelease);
    check_cancel(&cancel)?;
    let bytes = match read(
        RELEASE_API,
        MAX_RELEASE_BYTES,
        METADATA_TIMEOUT,
        cancel.clone(),
    ) {
        Ok(bytes) => bytes,
        Err(error) => {
            check_cancel(&cancel)?;
            if is_not_found(&error) {
                return Ok(manual(
                    MANUAL_RELEASE_URL,
                    "No public update catalog is available. Open the official releases page for a manual update.",
                ));
            }
            return Err(error).context("Cannot read the official GitHub release");
        }
    };
    check_cancel(&cancel)?;
    let release = Release::parse(&bytes)?;
    let Some(selected) = manifest_asset::select(&release.assets)? else {
        return Ok(manual(
            &release.html_url,
            "This release predates the update manifest. Open the official release for a manual update.",
        ));
    };
    let asset = selected.asset;
    ensure!(
        (1..=MAX_MANIFEST_BYTES as u64).contains(&asset.bytes()),
        "Manifest asset exceeds its 64 KiB bound or is empty"
    );
    progress(UpdateProgress::ReadingManifest);
    check_cancel(&cancel)?;
    let bytes = match read(
        asset.download_url(),
        asset.bytes() as usize,
        METADATA_TIMEOUT,
        cancel.clone(),
    ) {
        Ok(bytes) => bytes,
        Err(error) => {
            check_cancel(&cancel)?;
            if is_not_found(&error) {
                return Ok(manual(
                    &release.html_url,
                    "The update manifest is unavailable. Open the official release for a manual update.",
                ));
            }
            return Err(error).context("Cannot read the update manifest");
        }
    };
    check_cancel(&cancel)?;
    ensure!(
        bytes.len() as u64 == asset.bytes(),
        "Manifest byte count differs from the release asset"
    );
    if let Some(hash) = asset.sha256() {
        ensure!(
            sha256_hex(&bytes) == hash,
            "Manifest failed GitHub's asset SHA-256 check"
        );
    }
    let manifest = UpdateManifest::parse(&bytes)?;
    if let Some(revision) = selected.revision {
        ensure!(
            manifest.build_revision == revision,
            "Update manifest build revision does not match its asset filename"
        );
    }
    let installer = &manifest.platforms.windows_x86_64;
    validate_binding(
        &release.assets,
        &installer.installer_asset,
        &installer.installer_sha256,
        installer.installer_bytes,
    )?;
    for component in manifest.components.values() {
        validate_binding(
            &release.assets,
            &component.asset,
            &component.sha256,
            component.bytes,
        )?;
    }
    Ok(CatalogResult::Available(ReleaseCatalog {
        manifest: Box::new(manifest),
        release_url: release.html_url,
        assets: release.assets,
    }))
}

fn validate_binding(assets: &[ReleaseAsset], name: &str, hash: &str, bytes: u64) -> Result<()> {
    let asset = assets
        .iter()
        .find(|asset| asset.name() == name)
        .with_context(|| format!("Manifest names a missing release asset: {name}"))?;
    ensure!(
        asset.bytes() == bytes,
        "Manifest byte count differs from release asset {name}"
    );
    if let Some(digest) = asset.sha256() {
        ensure!(
            digest == hash,
            "Manifest SHA-256 differs from GitHub's asset digest for {name}"
        );
    }
    Ok(())
}

fn manual(url: &str, reason: &str) -> CatalogResult {
    CatalogResult::Manual {
        release_url: url.into(),
        reason: reason.into(),
    }
}

fn is_not_found(error: &anyhow::Error) -> bool {
    // The shared curl transport reports HTTP failures through its diagnostic tail.
    // Match the actual status diagnostic, not an arbitrary occurrence of "404".
    error.chain().any(|cause| {
        let message = cause.to_string();
        message.contains("requested URL returned error: 404") || message.trim() == "HTTP 404"
    })
}

pub(super) fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    Ok(())
}
