//! Select immutable manifest history by numeric revision, never by asset order.
use super::release::{MANIFEST_ASSET, ReleaseAsset};
use anyhow::{Context, Result, ensure};

const VERSIONED_PREFIX: &str = "UPDATE-MANIFEST-";

pub(super) struct SelectedManifest<'a> {
    pub asset: &'a ReleaseAsset,
    pub revision: Option<u64>,
}

pub(super) fn select(assets: &[ReleaseAsset]) -> Result<Option<SelectedManifest<'_>>> {
    let mut newest: Option<SelectedManifest<'_>> = None;
    for asset in assets {
        let Some(digits) = asset
            .name()
            .strip_prefix(VERSIONED_PREFIX)
            .and_then(|name| name.strip_suffix(".json"))
        else {
            continue;
        };
        ensure!(
            digits
                .as_bytes()
                .first()
                .is_some_and(|b| matches!(b, b'1'..=b'9'))
                && digits.bytes().all(|b| b.is_ascii_digit()),
            "Update manifest filename revision must be canonical positive decimal without leading zeros"
        );
        let revision = digits
            .parse::<u64>()
            .context("Update manifest filename revision exceeds u64")?;
        if let Some(previous) = &newest {
            ensure!(
                previous.revision != Some(revision),
                "Ambiguous update manifest revision"
            );
        }
        if newest
            .as_ref()
            .is_none_or(|previous| previous.revision < Some(revision))
        {
            newest = Some(SelectedManifest {
                asset,
                revision: Some(revision),
            });
        }
    }
    // Canonical decimal gives each revision one name. Release parsing also
    // rejects duplicate asset names/IDs before selection reaches this point.
    Ok(newest.or_else(|| {
        assets
            .iter()
            .find(|asset| asset.name() == MANIFEST_ASSET)
            .map(|asset| SelectedManifest {
                asset,
                revision: None,
            })
    }))
}
