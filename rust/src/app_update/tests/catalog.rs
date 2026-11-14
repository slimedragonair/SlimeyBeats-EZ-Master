use super::*;
use crate::app_update::{
    CatalogResult, MANUAL_RELEASE_URL, UpdateProgress,
    catalog::fetch_catalog_with,
    release::{MAX_RELEASE_BYTES, Release},
};
use std::sync::atomic::{AtomicUsize, Ordering};

fn fetch(fixture: &Fixture) -> Result<CatalogResult> {
    fetch_catalog_with(cancel(), &|_| {}, &|url, max, timeout, token| {
        fixture.read(url, max, timeout, token)
    })
}

#[test]
fn catalog_exposes_exact_name_asset_locators_and_verified_component_specs() {
    let mut manifest = manifest_fixture();
    manifest["components"] = json!({ "mcp_windows_x86_64": {
        "asset": "SlimeyBeats-Windows-Tools.zip", "sha256": "a".repeat(64), "bytes": 7,
        "protocol": 1, "files": { "mcp/slimeybeats-mcp.exe": "b".repeat(64) },
    }});
    let mut fixture = Fixture::new(manifest);
    fixture.release["assets"]
        .as_array_mut()
        .unwrap()
        .push(release_asset(
            103,
            "SlimeyBeats-Windows-Tools.zip",
            7,
            Some(&"a".repeat(64)),
        ));
    let CatalogResult::Available(catalog) = fetch(&fixture).unwrap() else {
        panic!("Expected catalog");
    };
    let component = &catalog.manifest().components["mcp_windows_x86_64"];
    let asset = catalog.asset(&component.asset).unwrap();
    assert_eq!(asset.id(), 103);
    assert_eq!(asset.bytes(), component.bytes);
    assert_eq!(asset.sha256(), Some(component.sha256.as_str()));
    assert!(
        asset.download_url().starts_with(
            "https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/"
        )
    );
    assert!(catalog.asset("../SlimeyBeats-Windows-Tools.zip").is_none());
    assert!(catalog.asset("slimeybeats-windows-tools.zip").is_none());
    assert_eq!(fixture.calls.borrow().len(), 2);
}

#[test]
fn latest_404_manifest_404_and_legacy_release_have_explicit_manual_routes() {
    let calls = AtomicUsize::new(0);
    let result = fetch_catalog_with(cancel(), &|_| {}, &|url, _, _, _| {
        calls.fetch_add(1, Ordering::Relaxed);
        assert_eq!(url, RELEASE_API);
        bail!("Decoder failed: curl: (22) The requested URL returned error: 404")
    })
    .unwrap();
    assert!(
        matches!(result, CatalogResult::Manual { release_url, .. } if release_url == MANUAL_RELEASE_URL)
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);

    let fixture = Fixture::new(manifest_fixture());
    let result = fetch_catalog_with(cancel(), &|_| {}, &|url, max, timeout, token| {
        if url == RELEASE_API {
            fixture.read(url, max, timeout, token)
        } else {
            bail!("HTTP 404")
        }
    })
    .unwrap();
    assert!(
        matches!(result, CatalogResult::Manual { reason, .. } if reason.contains("unavailable"))
    );

    let mut fixture = Fixture::new(manifest_fixture());
    fixture.release["assets"].as_array_mut().unwrap().remove(0);
    assert!(
        matches!(fetch(&fixture).unwrap(), CatalogResult::Manual { reason, .. } if reason.contains("predates"))
    );
    assert_eq!(fixture.calls.borrow().len(), 1);
}

#[test]
fn malformed_manifest_or_release_is_an_error_not_a_manual_success() {
    let mut fixture = Fixture::new(manifest_fixture());
    fixture.manifest = b"{bad-json}".to_vec();
    fixture.sync_manifest_asset();
    assert!(fetch(&fixture).is_err());
    assert!(
        fetch_catalog_with(cancel(), &|_| {}, &|_, _, _, _| Ok(
            b"{\"message\":\"Not Found\"}".to_vec()
        ))
        .is_err()
    );
    assert!(fetch_catalog_with(cancel(), &|_| {}, &|_, _, _, _| bail!("HTTP 403")).is_err());
    assert!(
        fetch_catalog_with(cancel(), &|_| {}, &|_, _, _, _| bail!(
            "rate limit 404 requests"
        ))
        .is_err()
    );
}

#[test]
fn url_traversal_cross_repo_spoofed_ids_and_duplicate_assets_are_rejected() {
    let fixture = Fixture::new(manifest_fixture());
    let valid = &fixture.release;
    for (pointer, value) in [
        (
            "/url",
            json!("https://api.github.com/repos/other/project/releases/42"),
        ),
        (
            "/html_url",
            json!(
                "https://github.com/slimedragonair/SlimeyBeats-EZ-Master.evil/releases/tag/v0.5.0-beta.3"
            ),
        ),
        (
            "/assets/1/url",
            json!(
                "https://api.github.com/repos/slimedragonair/SlimeyBeats-EZ-Master/releases/assets/999"
            ),
        ),
        (
            "/assets/1/browser_download_url",
            json!("https://github.com/other/project/releases/download/v0.5.0-beta.3/setup.exe"),
        ),
        (
            "/assets/1/browser_download_url",
            json!("https://github.com@evil.test/slimedragonair/SlimeyBeats-EZ-Master/setup.exe"),
        ),
        ("/assets/1/name", json!("../setup.exe")),
        ("/tag_name", json!("../v0.5.0-beta.3")),
        ("/id", json!(0)),
        ("/draft", json!(true)),
        ("/assets/1/state", json!("new")),
        ("/assets/1/id", json!(101)),
        ("/assets/1/name", json!(MANIFEST_ASSET)),
    ] {
        let mut release = valid.clone();
        *release.pointer_mut(pointer).unwrap() = value;
        assert!(
            Release::parse(&serde_json::to_vec(&release).unwrap()).is_err(),
            "{pointer}"
        );
    }
    let url = valid["assets"][1]["browser_download_url"].as_str().unwrap();
    for spoof in [
        url.replace("https:", "http:"),
        format!("{url}?next=https://evil.test"),
        url.replace("/download/", "/download/../"),
        url.replace("github.com", "github.com:443"),
        url.replace("Hotfix2", "Hotfix%32"),
    ] {
        let mut release = valid.clone();
        release["assets"][1]["browser_download_url"] = json!(spoof);
        assert!(Release::parse(&serde_json::to_vec(&release).unwrap()).is_err());
    }
}

#[test]
fn github_digests_and_exact_byte_sizes_must_agree_with_the_manifest() {
    for (pointer, value) in [
        (
            "/assets/1/digest",
            json!(format!("sha256:{}", "b".repeat(64))),
        ),
        ("/assets/1/digest", json!("sha256:bad")),
        ("/assets/1/digest", json!("md5:1234")),
        (
            "/assets/0/digest",
            json!(format!("sha256:{}", "b".repeat(64))),
        ),
        ("/assets/1/size", json!(4)),
        ("/assets/0/size", json!(1)),
        ("/assets/0/size", json!(65537)),
    ] {
        let mut fixture = Fixture::new(manifest_fixture());
        *fixture.release.pointer_mut(pointer).unwrap() = value;
        assert!(fetch(&fixture).is_err(), "{pointer}");
    }
    let mut fixture = Fixture::new(manifest_fixture());
    fixture.release["assets"][0]["digest"] = Value::Null;
    fixture.release["assets"][1]
        .as_object_mut()
        .unwrap()
        .remove("digest");
    assert!(matches!(
        fetch(&fixture).unwrap(),
        CatalogResult::Available(_)
    ));
    assert!(Release::parse(&vec![b' '; MAX_RELEASE_BYTES + 1]).is_err());
}

#[test]
fn missing_or_mismatched_component_assets_are_rejected() {
    let mut manifest = manifest_fixture();
    manifest["components"] = json!({ "vst3_windows_x86_64": {
        "asset": "SlimeyBeats-Windows-Tools.zip", "sha256": "a".repeat(64), "bytes": 7,
        "protocol": 1, "files": { "SlimeyBeats.vst3/Contents/x86_64-win/SlimeyBeats.vst3": "b".repeat(64) },
    }});
    let mut fixture = Fixture::new(manifest);
    assert!(fetch(&fixture).is_err());
    fixture.release["assets"]
        .as_array_mut()
        .unwrap()
        .push(release_asset(
            103,
            "SlimeyBeats-Windows-Tools.zip",
            8,
            Some(&"a".repeat(64)),
        ));
    assert!(fetch(&fixture).is_err());
    fixture.release["assets"][2]["size"] = json!(7);
    fixture.release["assets"][2]["digest"] = json!(format!("sha256:{}", "b".repeat(64)));
    assert!(fetch(&fixture).is_err());
}

#[test]
fn cancellation_is_checked_before_network_and_between_metadata_steps() {
    let token = Arc::new(AtomicBool::new(true));
    assert!(
        fetch_catalog_with(token, &|_| {}, &|_, _, _, _| panic!("No HTTP after cancel")).is_err()
    );
    let fixture = Fixture::new(manifest_fixture());
    let token = cancel();
    let result = fetch_catalog_with(
        token.clone(),
        &|stage| {
            if stage == UpdateProgress::ReadingManifest {
                token.store(true, Ordering::Relaxed);
            }
        },
        &|url, max, timeout, cancellation| fixture.read(url, max, timeout, cancellation),
    );
    assert!(result.is_err());
    assert_eq!(fixture.calls.borrow().len(), 1);
}

#[test]
fn linux_and_unsupported_architectures_never_offer_or_fetch_an_installer() {
    for platform in ["linux_x86_64", "windows_aarch64", "macos_aarch64", "manual"] {
        let result = check_with(
            &super::super::current(),
            platform,
            cancel(),
            &|_| {},
            &|_, _, _, _| panic!("Unsupported target must not request an installer"),
        )
        .unwrap();
        assert!(matches!(result, CheckResult::Manual { release_url, reason }
            if release_url == MANUAL_RELEASE_URL && reason.contains("Windows x86_64 only")));
    }
}
