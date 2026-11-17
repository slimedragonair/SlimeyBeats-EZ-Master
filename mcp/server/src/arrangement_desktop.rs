//! Probe only an explicitly requested arrangement handoff, never ordinary discovery.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::Path,
    process::Command,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

const MAX_CAPABILITY_BYTES: usize = 8192;
const CAPABILITY_TIMEOUT: Duration = Duration::from_secs(3);
const ACTION: &str = "Desktop v2 capability verification failed. Build or install the matching preview desktop with --studio-project-capabilities support, then configure --desktop-exe to that build. Matching engine/version or build labels alone do not establish v2 support. No arrangement project was saved or handed off";

#[derive(Deserialize, Serialize)]
struct Capabilities {
    format: String,
    version: u32,
    engine_version: String,
    studio_project_versions: Vec<u32>,
    arrangement_live_editing: bool,
    autoplay_on_project_open: bool,
}

fn validate(bytes: &[u8]) -> Result<Value> {
    ensure!(
        bytes.len() <= MAX_CAPABILITY_BYTES,
        "Desktop capability response exceeds 8 KiB"
    );
    let caps: Capabilities = serde_json::from_slice(bytes)
        .context("Desktop returned incomplete or malformed capability JSON")?;
    ensure!(
        caps.format == "slimeybeats-desktop-project-capabilities" && caps.version == 1,
        "Unsupported desktop project capability format/version"
    );
    ensure!(
        !caps.engine_version.is_empty()
            && caps.engine_version.len() <= 128
            && !caps.engine_version.chars().any(char::is_control),
        "Invalid desktop engine version metadata"
    );
    ensure!(
        caps.studio_project_versions.contains(&2),
        "Desktop cannot read canonical v2 Studio projects"
    );
    ensure!(
        caps.arrangement_live_editing && !caps.autoplay_on_project_open,
        "Desktop must support live arrangement editing and paused project opening"
    );
    Ok(serde_json::to_value(caps)?)
}

pub(crate) fn require_v2(executable: &Path, cancel: Arc<AtomicBool>) -> Result<Value> {
    (|| {
        let mut command = Command::new(executable);
        command.arg("--studio-project-capabilities");
        let mut bytes = Vec::new();
        // Shared supervision bounds output/time and kills/reaps the owned process
        // tree, including a worker that closes stdout and then stalls.
        crate::codec_process::read_stdout(
            command,
            MAX_CAPABILITY_BYTES,
            CAPABILITY_TIMEOUT,
            cancel,
            |chunk| {
                bytes.extend_from_slice(chunk);
                Ok(())
            },
        )?;
        validate(&bytes)
    })()
    .context(ACTION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn supported() -> Value {
        json!({"format":"slimeybeats-desktop-project-capabilities","version":1,
            "engine_version":"0.5.0-beta.3","studio_project_versions":[1,2],
            "arrangement_live_editing":true,"autoplay_on_project_open":false})
    }

    #[test]
    fn complete_explicit_v2_contract_is_required_even_for_the_same_engine_version() {
        let caps = supported();
        assert_eq!(validate(&serde_json::to_vec(&caps).unwrap()).unwrap(), caps);
        for key in [
            "format",
            "version",
            "engine_version",
            "studio_project_versions",
            "arrangement_live_editing",
            "autoplay_on_project_open",
        ] {
            let mut incomplete = caps.clone();
            incomplete.as_object_mut().unwrap().remove(key);
            assert!(
                validate(&serde_json::to_vec(&incomplete).unwrap()).is_err(),
                "Missing {key} accepted"
            );
        }
        for (key, value) in [
            ("studio_project_versions", json!([1])),
            ("arrangement_live_editing", json!(false)),
            ("autoplay_on_project_open", json!(true)),
            ("format", json!("other")),
            ("version", json!(2)),
        ] {
            let mut old = caps.clone();
            old[key] = value;
            assert!(
                validate(&serde_json::to_vec(&old).unwrap()).is_err(),
                "Invalid {key} accepted"
            );
        }
    }

    #[test]
    fn malformed_non_json_and_excessive_responses_are_rejected() {
        for bytes in [
            b"0.5.0-beta.3 Build 1501".as_slice(),
            b"{broken",
            b"[]",
            b"null",
        ] {
            assert!(validate(bytes).is_err());
        }
        assert!(validate(&vec![b' '; MAX_CAPABILITY_BYTES + 1]).is_err());
    }
}
