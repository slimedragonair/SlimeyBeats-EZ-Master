use super::{identity::BuildIdentity, manifest::WindowsInstaller};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub enum CheckResult {
    Available(UpdateCandidate),
    UpToDate { latest: BuildIdentity },
    Manual { release_url: String, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateProgress {
    CheckingRelease,
    ReadingManifest,
    Downloading {
        received_bytes: u64,
        total_bytes: u64,
    },
    Verifying,
    Ready,
}

/// Constructed only after validating the official release and its manifest.
#[derive(Clone, Debug)]
pub struct UpdateCandidate {
    pub(super) identity: BuildIdentity,
    pub(super) notes: String,
    pub(super) release_url: String,
    pub(super) installer_url: String,
    pub(super) installer: WindowsInstaller,
}

impl UpdateCandidate {
    pub fn identity(&self) -> &BuildIdentity {
        &self.identity
    }

    pub fn version(&self) -> &str {
        &self.identity.version
    }

    pub fn build_revision(&self) -> u64 {
        self.identity.build_revision
    }

    pub fn channel(&self) -> &str {
        &self.identity.channel
    }

    pub fn notes(&self) -> &str {
        &self.notes
    }

    pub fn release_url(&self) -> &str {
        &self.release_url
    }

    pub fn platform(&self) -> &str {
        super::WINDOWS_X86_64
    }

    pub fn installer_asset(&self) -> &str {
        &self.installer.installer_asset
    }

    pub fn installer_sha256(&self) -> &str {
        &self.installer.installer_sha256
    }

    pub fn installer_bytes(&self) -> u64 {
        self.installer.installer_bytes
    }

    pub fn executable_sha256(&self) -> &str {
        &self.installer.executable_sha256
    }
}

/// Verified cache artifact only; the caller owns explicit installation consent.
#[derive(Clone, Debug)]
pub struct VerifiedInstaller {
    pub path: PathBuf,
    pub identity: BuildIdentity,
    pub platform: String,
    pub installer_sha256: String,
    pub installer_bytes: u64,
    pub executable_sha256: String,
    pub release_url: String,
}
