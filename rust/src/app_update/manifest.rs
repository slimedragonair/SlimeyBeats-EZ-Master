//! Schema, identity and extraction-entry validation; no network or installation.
use super::{
    identity::{BuildIdentity, CHANNEL},
    release::validate_asset_name,
};
use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::{
    Deserialize,
    de::{MapAccess, Visitor},
};
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fmt,
};

pub(super) const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub(super) const MAX_INSTALLER_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_NOTES_BYTES: usize = 16 * 1024;
const MAX_COMPONENTS: usize = 16;
const MAX_COMPONENT_FILES: usize = 256;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateManifest {
    pub schema: u32,
    pub version: String,
    pub build_revision: u64,
    pub channel: String,
    pub notes: String,
    pub platforms: ManifestPlatforms,
    /// Optional recovery bundles; installers normally include these helpers.
    #[serde(default, deserialize_with = "unique_map")]
    pub components: BTreeMap<String, ComponentSpec>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestPlatforms {
    pub windows_x86_64: WindowsInstaller,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsInstaller {
    pub installer_asset: String,
    pub installer_sha256: String,
    pub installer_bytes: u64,
    pub executable_sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentSpec {
    pub asset: String,
    pub sha256: String,
    pub bytes: u64,
    /// Exact relative ZIP entries expected by the parent's component installer.
    #[serde(deserialize_with = "unique_map")]
    pub files: BTreeMap<String, String>,
    /// The component consumer must also check the protocol it implements.
    pub protocol: u32,
}

impl UpdateManifest {
    pub fn identity(&self) -> BuildIdentity {
        BuildIdentity {
            version: self.version.clone(),
            build_revision: self.build_revision,
            channel: self.channel.clone(),
        }
    }

    pub(super) fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_MANIFEST_BYTES,
            "Update manifest exceeds its 64 KiB bound or is empty"
        );
        let mut manifest: Self =
            serde_json::from_slice(bytes).context("Invalid update manifest JSON")?;
        parse_version(&manifest.version)?;
        ensure!(manifest.schema == 1, "Unsupported update manifest schema");
        ensure!(
            manifest.channel == CHANNEL,
            "Update manifest is for a different channel"
        );
        ensure!(
            manifest.build_revision > 0,
            "Update build revision must be positive"
        );
        ensure!(
            manifest.notes.len() <= MAX_NOTES_BYTES
                && !manifest
                    .notes
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
            "Update notes exceed their bound or contain control characters"
        );
        let installer = &mut manifest.platforms.windows_x86_64;
        validate_asset_name(&installer.installer_asset)?;
        let prefix = format!("SlimeyBeats-EZ-Master-{}", manifest.version);
        let label = installer
            .installer_asset
            .strip_suffix("-Setup.exe")
            .and_then(|stem| stem.strip_prefix(&prefix))
            .context("Installer filename must match the manifest version and end in -Setup.exe")?;
        ensure!(
            label.is_empty() || (label.starts_with('-') && label.len() > 1),
            "Installer filename has an invalid release label"
        );
        validate_size(installer.installer_bytes)?;
        installer.installer_sha256 = normalize_hash(&installer.installer_sha256)?;
        installer.executable_sha256 = normalize_hash(&installer.executable_sha256)?;
        ensure!(
            manifest.components.len() <= MAX_COMPONENTS,
            "Too many update components"
        );
        for (name, component) in &mut manifest.components {
            validate_asset_name(name)?;
            ensure!(name.len() <= 64, "Component name exceeds its bound");
            validate_asset_name(&component.asset)?;
            ensure!(
                component.asset.ends_with(".zip"),
                "Component assets must be ZIP bundles"
            );
            validate_size(component.bytes)?;
            component.sha256 = normalize_hash(&component.sha256)?;
            ensure!(
                component.protocol > 0,
                "Component protocol must be positive"
            );
            ensure!(
                !component.files.is_empty() && component.files.len() <= MAX_COMPONENT_FILES,
                "Component file list is empty or exceeds its bound"
            );
            let mut entries = BTreeSet::new();
            for (entry, hash) in &mut component.files {
                validate_entry(entry)?;
                ensure!(
                    entries.insert(entry.to_ascii_lowercase()),
                    "Component file entries collide on Windows"
                );
                *hash = normalize_hash(hash)?;
            }
            for entry in &entries {
                for (separator, _) in entry.match_indices('/') {
                    ensure!(
                        !entries.contains(&entry[..separator]),
                        "Component entries conflict as both a file and a directory"
                    );
                }
            }
        }
        Ok(manifest)
    }
}

pub(super) fn validate_size(bytes: u64) -> Result<()> {
    ensure!(
        (1..=MAX_INSTALLER_BYTES).contains(&bytes),
        "Update asset size must be between 1 byte and 1 GiB"
    );
    Ok(())
}

pub(super) fn normalize_hash(hash: &str) -> Result<String> {
    ensure!(
        hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid update SHA-256"
    );
    Ok(hash.to_ascii_lowercase())
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn parse_version(version: &str) -> Result<Version> {
    ensure!(version.len() <= 128, "Update version exceeds its bound");
    Version::parse(version).context("Update version is not strict SemVer")
}

/// Both dimensions are monotonic. Build metadata does not affect SemVer precedence.
pub(super) fn is_newer(incoming: &BuildIdentity, installed: &BuildIdentity) -> Result<bool> {
    ensure!(
        incoming.channel == CHANNEL && installed.channel == CHANNEL,
        "Cannot update across installation channels"
    );
    ensure!(
        incoming.build_revision > 0 && installed.build_revision > 0,
        "Invalid update build identity"
    );
    let precedence =
        parse_version(&incoming.version)?.cmp_precedence(&parse_version(&installed.version)?);
    ensure!(
        precedence != Ordering::Less,
        "The published version is older; refusing a downgrade"
    );
    ensure!(
        incoming.build_revision >= installed.build_revision,
        "The published build revision is older; refusing a downgrade"
    );
    if precedence == Ordering::Greater {
        ensure!(
            incoming.build_revision > installed.build_revision,
            "A newer version must also increase its build revision"
        );
    }
    Ok(precedence == Ordering::Greater || incoming.build_revision > installed.build_revision)
}

fn validate_entry(entry: &str) -> Result<()> {
    ensure!(
        !entry.is_empty() && entry.len() <= 240,
        "Invalid component file entry length"
    );
    for segment in entry.split('/') {
        validate_asset_name(segment)?;
        ensure!(
            !segment.ends_with('.'),
            "Component file entries cannot end in a dot"
        );
        let stem = segment.split('.').next().unwrap_or("").to_ascii_uppercase();
        ensure!(
            !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9')),
            "Component file entry uses a reserved Windows name"
        );
    }
    Ok(())
}

// Serde's default map reader accepts duplicate keys; manifests must not be ambiguous.
fn unique_map<'de, D, T>(deserializer: D) -> std::result::Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct UniqueMap<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for UniqueMap<T> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an object without duplicate keys")
        }
        fn visit_map<M: MapAccess<'de>>(
            self,
            mut access: M,
        ) -> std::result::Result<Self::Value, M::Error> {
            let mut map = BTreeMap::new();
            while let Some((key, value)) = access.next_entry::<String, T>()? {
                if map.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("Duplicate manifest map key"));
                }
            }
            Ok(map)
        }
    }
    deserializer.deserialize_map(UniqueMap(std::marker::PhantomData))
}
