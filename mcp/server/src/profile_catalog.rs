//! Snapshot owner-authorized custom profiles without accepting tool-supplied paths.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, io::Read, path::Path};

const MAX_PROFILE_BYTES: usize = 64 * 1024;
const MAX_CUSTOM_BYTES: usize = 8 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
pub(crate) const MAX_PROFILE_ID_BYTES: usize = 256;

pub struct ProfileCatalog {
    profiles: BTreeMap<String, Value>,
    configured: bool,
    warnings: Vec<Value>,
}

impl ProfileCatalog {
    pub fn load(root: Option<&Path>) -> Result<Self> {
        let mut catalog = Self {
            profiles: mastercraft::profiles::builtin(),
            configured: root.is_some(),
            warnings: Vec::new(),
        };
        let Some(root) = root else {
            return Ok(catalog);
        };
        let root = match root.canonicalize() {
            Ok(root) => root,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(catalog),
            Err(error) => return Err(error).context("Cannot read the configured profiles root"),
        };
        ensure!(root.is_dir(), "--profiles-root must identify a directory");
        let mut bytes = 0;
        for (index, entry) in fs::read_dir(&root)?.enumerate() {
            ensure!(
                index < MAX_ENTRIES,
                "Profile directory exceeds its 4096-entry scan limit"
            );
            let path = entry?.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let id = format!("custom:{name}");
            let loaded = (|| -> Result<Value> {
                ensure!(
                    !name.is_empty()
                        && id.len() <= MAX_PROFILE_ID_BYTES
                        && name.trim() == name
                        && !name.contains(['/', '\\', ':', '+'])
                        && name != "."
                        && name != "..",
                    "Profile filename cannot form a safe custom ID"
                );
                let selected = path.canonicalize()?;
                ensure!(
                    selected.starts_with(&root) && selected.is_file(),
                    "Profile file escapes the configured root or is not a file"
                );
                let remaining = MAX_CUSTOM_BYTES
                    .checked_sub(bytes)
                    .context("Custom profile data exceeds its 8 MiB budget")?;
                let mut text = String::new();
                fs::File::open(selected)?
                    .take(MAX_PROFILE_BYTES.min(remaining) as u64 + 1)
                    .read_to_string(&mut text)?;
                ensure!(
                    text.len() <= MAX_PROFILE_BYTES,
                    "Profile exceeds its 64 KiB byte limit"
                );
                bytes = bytes
                    .checked_add(text.len())
                    .context("Profile byte count overflow")?;
                ensure!(
                    bytes <= MAX_CUSTOM_BYTES,
                    "Custom profile data exceeds its 8 MiB budget"
                );
                let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
                let bands = mastercraft::profiles::target_curve(&value)?;
                ensure!(
                    bands.len() == 10 && bands.iter().all(|value| value.is_finite()),
                    "Custom profile needs ten finite tonal_db values"
                );
                Ok(value)
            })();
            match loaded {
                Ok(value) => {
                    catalog.profiles.insert(id, value);
                }
                Err(error) => catalog
                    .warnings
                    .push(json!({"id":id,"error":format!("{error:#}")})),
            }
            ensure!(
                bytes <= MAX_CUSTOM_BYTES,
                "Custom profile data exceeds its 8 MiB budget"
            );
        }
        Ok(catalog)
    }

    pub fn resolve(&self, name: &str) -> Result<Value> {
        ensure!(
            !name.is_empty() && name.len() <= MAX_PROFILE_ID_BYTES,
            "Choose a profile ID from get_capabilities"
        );
        if name.starts_with("custom:") {
            return self.profiles.get(name).cloned().context("Unknown authorized custom profile; inspect get_capabilities for IDs. Only --profiles-root authorizes custom files");
        }
        crate::options::profile(name)
    }

    pub fn describe(&self) -> Vec<Value> {
        self.profiles.iter().map(|(id, value)| json!({"id":id,"name":value["display_name"].as_str().unwrap_or(id),"targets_lufs":value["lufs"],"source":if id.starts_with("custom:") {"owner_configured"} else {"bundled"}})).collect()
    }

    pub fn policy(&self) -> Value {
        json!({"custom_root_configured":self.configured,"custom_ids":"Use exact custom:<filename stem> IDs from profiles[]. No arbitrary file paths or custom-profile blends. Bundled weighted blends remain supported.","snapshot":"Custom JSON is read once at connection startup. Reconnect explicitly after editing profiles; no client configuration is edited automatically.","max_profile_bytes":MAX_PROFILE_BYTES,"max_custom_bytes":MAX_CUSTOM_BYTES,"max_directory_entries":MAX_ENTRIES,"warnings":self.warnings})
    }
}

#[cfg(test)]
#[path = "profile_catalog_tests.rs"]
mod tests;
