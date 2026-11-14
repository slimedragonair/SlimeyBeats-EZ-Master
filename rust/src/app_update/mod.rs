//! Explicit, download-only application updates from the official GitHub release.
//!
//! Integration: register this module on non-Wasm targets and add `semver = "1"`.
//! `current()` reports the running build. Run `check(cancel, progress)` on a worker;
//! only `CheckResult::Available` contains a validated, downloadable candidate.
//! `download(&candidate, cache_root, cancel, progress)` returns an inert installer
//! and its executable fingerprint for the user-confirmed interactive installer.
//! This module never launches an installer, changes preferences, or touches audio.
//!
//! Schema 1 is described by `UPDATE-MANIFEST.schema.json`. Append an immutable
//! `UPDATE-MANIFEST-<build_revision>.json` beside each new installer, incrementing
//! `BUILD_REVISION` even when a hotfix retains the same package version. Selection
//! uses the highest canonical positive decimal u64 filename revision and requires
//! the payload revision to match. Malformed versioned names are rejected. The
//! legacy `UPDATE-MANIFEST.json` is used only without any versioned manifests;
//! failure of the selected manifest never retries older catalogs or the legacy
//! alias. The fixed release API supplies asset IDs and URLs; no URLs are accepted
//! in the manifest, and earlier release assets never need replacement or removal.
//! Missing manifests and unsupported targets return an explicit manual route.
//! `fetch_catalog` exposes the same read-only manifest and exact-name asset
//! locators for optional helper recovery. Recommended component IDs are
//! `mcp_windows_x86_64` and `vst3_windows_x86_64`; each component has `asset`,
//! `sha256`, `bytes`, `files` (entry to hash), and `protocol`. The consumer owns
//! protocol checks, safe extraction and explicit activation of those helpers.
//! Call `VerifiedInstaller::verify(cancel)` again just before opening NSIS.
//! Transport, byte bounds, cancellation and atomic SHA-256 publication are shared
//! through `crate::downloads`; schema/release validation stays in this module.

mod catalog;
mod check;
mod download;
pub mod identity;
mod manifest;
mod manifest_asset;
mod release;
mod types;

pub use catalog::{CatalogResult, ReleaseCatalog, fetch_catalog};
pub use check::check;
pub use download::download;
pub use identity::{BuildIdentity, current};
pub use manifest::{ComponentSpec, ManifestPlatforms, UpdateManifest, WindowsInstaller};
pub use release::ReleaseAsset;
pub use types::{CheckResult, UpdateCandidate, UpdateProgress, VerifiedInstaller};

/// Public release page for manual updates, including Linux source builds.
pub const MANUAL_RELEASE_URL: &str =
    "https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/latest";

pub const WINDOWS_X86_64: &str = "windows_x86_64";

#[cfg(test)]
mod tests;

/// GUI regression fixtures can describe historical builds without weakening
/// production candidate eligibility or exposing a production constructor.
#[cfg(test)]
pub(crate) fn fixture_candidate(revision: u64, sha: &str) -> UpdateCandidate {
    assert!(revision > 0, "Fixture revision must be positive");
    let mut candidate = tests::available_candidate();
    candidate.identity.build_revision = revision;
    candidate.installer.installer_sha256 =
        manifest::normalize_hash(sha).expect("Valid fixture SHA-256");
    candidate
}
