mod catalog;
mod download;
mod manifest;
mod manifest_selection;
mod versions;

use super::{
    CheckResult, UpdateCandidate, WINDOWS_X86_64,
    check::check_with,
    manifest::sha256_hex,
    release::{MANIFEST_ASSET, RELEASE_API},
};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

fn cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn manifest_fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/update-manifest.json")).unwrap()
}

struct Fixture {
    release: Value,
    manifest: Vec<u8>,
    calls: RefCell<Vec<String>>,
}

impl Fixture {
    fn new(manifest: Value) -> Self {
        let installer = &manifest["platforms"]["windows_x86_64"];
        let mut release: Value =
            serde_json::from_str(include_str!("fixtures/release.json")).unwrap();
        release["assets"] = json!([null, null]);
        release["assets"][1] = release_asset(
            102,
            installer["installer_asset"].as_str().unwrap(),
            installer["installer_bytes"].as_u64().unwrap(),
            installer["installer_sha256"].as_str(),
        );
        let mut fixture = Self {
            release,
            manifest: serde_json::to_vec(&manifest).unwrap(),
            calls: RefCell::new(Vec::new()),
        };
        fixture.sync_manifest_asset();
        fixture
    }

    fn sync_manifest_asset(&mut self) {
        self.release["assets"][0] = release_asset(
            101,
            MANIFEST_ASSET,
            self.manifest.len() as u64,
            Some(&sha256_hex(&self.manifest)),
        );
    }

    fn read(
        &self,
        url: &str,
        max: usize,
        timeout: Duration,
        cancellation: Arc<AtomicBool>,
    ) -> Result<Vec<u8>> {
        assert_eq!(timeout, Duration::from_secs(30));
        assert!(!cancellation.load(std::sync::atomic::Ordering::Relaxed));
        self.calls.borrow_mut().push(url.into());
        if url == RELEASE_API {
            assert_eq!(max, super::release::MAX_RELEASE_BYTES);
            Ok(serde_json::to_vec(&self.release).unwrap())
        } else if url
            == self.release["assets"][0]["browser_download_url"]
                .as_str()
                .unwrap()
        {
            assert_eq!(
                max,
                self.release["assets"][0]["size"].as_u64().unwrap() as usize
            );
            Ok(self.manifest.clone())
        } else {
            bail!("Unexpected test metadata URL: {url}")
        }
    }
}

fn release_asset(id: u64, name: &str, bytes: u64, hash: Option<&str>) -> Value {
    json!({
        "id": id, "name": name, "state": "uploaded", "size": bytes,
        "url": format!("https://api.github.com/repos/slimedragonair/SlimeyBeats-EZ-Master/releases/assets/{id}"),
        "browser_download_url": format!("https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/v0.5.0-beta.3/{name}"),
        "digest": hash.map(|hash| format!("sha256:{hash}")),
    })
}

pub(super) fn available_candidate() -> UpdateCandidate {
    let mut manifest = manifest_fixture();
    // This fixture represents a future update even after a same-version hotfix.
    manifest["build_revision"] = json!(super::identity::BUILD_REVISION.checked_add(1).unwrap());
    let fixture = Fixture::new(manifest);
    let result = check_with(
        &super::current(),
        WINDOWS_X86_64,
        cancel(),
        &|_| {},
        &|url, max, timeout, token| fixture.read(url, max, timeout, token),
    )
    .unwrap();
    match result {
        CheckResult::Available(candidate) => candidate,
        other => panic!("Expected candidate: {other:?}"),
    }
}
