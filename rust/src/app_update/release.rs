//! Exact official repository URL and GitHub asset-identity validation.
use super::manifest::normalize_hash;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::BTreeSet;

pub(super) const RELEASE_API: &str =
    "https://api.github.com/repos/slimedragonair/SlimeyBeats-EZ-Master/releases/latest";
pub(super) const MANIFEST_ASSET: &str = "UPDATE-MANIFEST.json";
pub(super) const MAX_RELEASE_BYTES: usize = 2 * 1024 * 1024;
const API_REPOSITORY: &str = "https://api.github.com/repos/slimedragonair/SlimeyBeats-EZ-Master";
const WEB_REPOSITORY: &str = "https://github.com/slimedragonair/SlimeyBeats-EZ-Master";

#[derive(Debug)]
pub(super) struct Release {
    pub(super) html_url: String,
    pub(super) assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ApiRelease {
    id: u64,
    url: String,
    html_url: String,
    tag_name: String,
    draft: bool,
    assets: Vec<ApiAsset>,
}

/// Read-only asset locator authenticated by the fixed repository's release API.
#[derive(Clone, Debug)]
pub struct ReleaseAsset {
    id: u64,
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

#[derive(Deserialize)]
struct ApiAsset {
    id: u64,
    name: String,
    url: String,
    browser_download_url: String,
    size: u64,
    state: String,
    #[serde(default)]
    digest: Option<String>,
}

impl ReleaseAsset {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn download_url(&self) -> &str {
        &self.browser_download_url
    }
    pub fn bytes(&self) -> u64 {
        self.size
    }
    /// GitHub's SHA-256 when supplied; the manifest hash is still mandatory.
    pub fn sha256(&self) -> Option<&str> {
        self.digest.as_deref()
    }
}

impl Release {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_RELEASE_BYTES,
            "Release metadata exceeds its 2 MiB bound or is empty"
        );
        let mut release: ApiRelease =
            serde_json::from_slice(bytes).context("Invalid GitHub release metadata JSON")?;
        ensure!(
            release.id > 0 && !release.draft,
            "Update release is not published"
        );
        ensure!(
            release.url == format!("{API_REPOSITORY}/releases/{}", release.id),
            "Release API URL is outside the official repository"
        );
        validate_asset_name(&release.tag_name)?;
        ensure!(
            release.tag_name.len() <= 128,
            "Release tag exceeds its bound"
        );
        ensure!(
            release.html_url == format!("{WEB_REPOSITORY}/releases/tag/{}", release.tag_name),
            "Release page is outside the official repository"
        );
        ensure!(
            release.assets.len() <= 256,
            "Release contains too many assets"
        );
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for asset in &mut release.assets {
            validate_asset_name(&asset.name)?;
            ensure!(
                asset.id > 0 && ids.insert(asset.id) && names.insert(asset.name.clone()),
                "Duplicate or invalid release asset identity"
            );
            ensure!(
                asset.state == "uploaded",
                "Release asset is not completely uploaded"
            );
            ensure!(
                asset.url == format!("{API_REPOSITORY}/releases/assets/{}", asset.id),
                "Asset API URL is outside the official repository"
            );
            ensure!(
                asset.browser_download_url
                    == format!(
                        "{WEB_REPOSITORY}/releases/download/{}/{}",
                        release.tag_name, asset.name
                    ),
                "Asset download URL does not match the official release and filename"
            );
            if let Some(digest) = &asset.digest {
                asset.digest = Some(normalize_hash(
                    digest
                        .strip_prefix("sha256:")
                        .context("Unsupported GitHub asset digest algorithm")?,
                )?);
            }
        }
        Ok(Self {
            html_url: release.html_url,
            assets: release
                .assets
                .into_iter()
                .map(|asset| ReleaseAsset {
                    id: asset.id,
                    name: asset.name,
                    browser_download_url: asset.browser_download_url,
                    size: asset.size,
                    digest: asset.digest,
                })
                .collect(),
        })
    }
}

pub(super) fn validate_asset_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 200
            && name.as_bytes()[0].is_ascii_alphanumeric()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'+'))
            && !name.contains(".."),
        "Unsafe release asset name or path"
    );
    Ok(())
}
