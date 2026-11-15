use super::*;
use crate::app_update::manifest::{MAX_INSTALLER_BYTES, MAX_MANIFEST_BYTES, UpdateManifest};

fn parse(value: Value) -> Result<UpdateManifest> {
    UpdateManifest::parse(&serde_json::to_vec(&value).unwrap())
}

fn with_component() -> Value {
    let mut manifest = manifest_fixture();
    manifest["components"] = json!({ "mcp_windows_x86_64": {
        "asset": "SlimeyBeats-Windows-Tools.zip", "sha256": "a".repeat(64), "bytes": 3,
        "protocol": 1, "files": { "mcp/slimeybeats-mcp.exe": "b".repeat(64) },
    }});
    manifest
}

#[test]
fn fixtures_support_default_components_and_normalized_hashes() {
    assert!(parse(manifest_fixture()).unwrap().components.is_empty());
    let mut manifest = with_component();
    manifest["platforms"]["windows_x86_64"]["executable_sha256"] = json!("A".repeat(64));
    manifest["components"]["mcp_windows_x86_64"]["sha256"] = json!("A".repeat(64));
    let parsed = parse(manifest).unwrap();
    assert_eq!(
        parsed.platforms.windows_x86_64.executable_sha256,
        "a".repeat(64)
    );
    assert_eq!(
        parsed.components["mcp_windows_x86_64"].sha256,
        "a".repeat(64)
    );
}

#[test]
fn malformed_json_unknown_fields_and_wrong_schema_are_rejected() {
    for body in [
        b"not json".as_slice(),
        b"{}",
        b"[]",
        b"null",
        b"{\"schema\":1,\"schema\":1}",
    ] {
        assert!(UpdateManifest::parse(body).is_err());
    }
    for (pointer, value) in [
        ("/schema", json!(2)),
        ("/schema", json!(1.0)),
        ("/channel", json!("stable")),
        ("/build_revision", json!(0)),
        ("/build_revision", json!(-1)),
        ("/build_revision", json!(1.5)),
        ("/build_revision", json!("2026111403")),
    ] {
        let mut manifest = manifest_fixture();
        *manifest.pointer_mut(pointer).unwrap() = value;
        assert!(parse(manifest).is_err(), "{pointer}");
    }
    let mut manifest = manifest_fixture();
    manifest["url"] = json!("https://example.test/setup.exe");
    assert!(parse(manifest).is_err());
    let mut manifest = manifest_fixture();
    manifest["platforms"]["linux_x86_64"] = json!({});
    assert!(parse(manifest).is_err());
    let fixture = manifest_fixture();
    let revision = fixture["build_revision"].as_u64().unwrap().to_string();
    let overflow = serde_json::to_string(&fixture)
        .unwrap()
        .replace(revision.as_str(), "18446744073709551616");
    assert!(UpdateManifest::parse(overflow.as_bytes()).is_err());
}

#[test]
fn strict_semver_rejects_ambiguous_or_non_numeric_versions() {
    for version in [
        "v0.5.0",
        "0.5",
        "0.05.0",
        "0.5.0-beta.01",
        "0.5.0-",
        "0.5.0+",
        " 0.5.0",
        "999999999999999999999999.0.0",
    ] {
        let mut manifest = manifest_fixture();
        manifest["version"] = json!(version);
        assert!(parse(manifest).is_err(), "{version}");
    }
}

#[test]
fn traversal_urls_nonmatching_versions_and_bad_hashes_are_rejected() {
    for name in [
        "../Setup.exe",
        "SlimeyBeats-EZ-Master-0.5.0-beta.3/../../Setup.exe",
        "SlimeyBeats-EZ-Master-0.5.0-beta.3%2fSetup.exe",
        "https://example.test/Setup.exe",
        "SlimeyBeats-EZ-Master-0.5.0-beta.3\\Setup.exe",
        "C:Setup.exe",
        "SlimeyBeats-EZ-Master-0.5.0-beta.30-Setup.exe",
        "SlimeyBeats-EZ-Master-0.5.0-beta.3-Setup.exe:evil",
        "SlimeyBeats-EZ-Master-0.5.0-beta.3-..-Setup.exe",
        "SlimeyBeats-EZ-Master-0.5.0-beta.3-Setup.msi",
    ] {
        let mut manifest = manifest_fixture();
        manifest["platforms"]["windows_x86_64"]["installer_asset"] = json!(name);
        assert!(parse(manifest).is_err(), "{name}");
    }
    for field in ["installer_sha256", "executable_sha256"] {
        for hash in [
            "a".repeat(63),
            "a".repeat(65),
            "g".repeat(64),
            format!("{}\n", "a".repeat(63)),
        ] {
            let mut manifest = manifest_fixture();
            manifest["platforms"]["windows_x86_64"][field] = json!(hash);
            assert!(parse(manifest).is_err());
        }
    }
}

#[test]
fn manifest_notes_and_asset_bounds_are_enforced() {
    assert!(UpdateManifest::parse(&vec![b' '; MAX_MANIFEST_BYTES + 1]).is_err());
    for bytes in [0, MAX_INSTALLER_BYTES + 1, u64::MAX] {
        let mut manifest = manifest_fixture();
        manifest["platforms"]["windows_x86_64"]["installer_bytes"] = json!(bytes);
        assert!(parse(manifest).is_err());
    }
    for notes in ["a".repeat(16385), "\u{0}".into(), "é".repeat(8193)] {
        let mut manifest = manifest_fixture();
        manifest["notes"] = json!(notes);
        assert!(parse(manifest).is_err());
    }
}

#[test]
fn components_reject_traversal_windows_aliases_and_ambiguous_entries() {
    for entry in [
        "../worker.exe",
        "/worker.exe",
        "C:/worker.exe",
        "mcp\\worker.exe",
        "mcp//worker.exe",
        "./worker.exe",
        "mcp/CON.exe",
        "mcp/lpt1.dll",
        "mcp/worker.exe.",
        "mcp/worker.exe:stream",
        "mcp/%2e%2e/worker.exe",
        "mcp/worker .exe",
    ] {
        let mut manifest = with_component();
        manifest["components"]["mcp_windows_x86_64"]["files"] = json!({entry: "a".repeat(64)});
        assert!(parse(manifest).is_err(), "{entry}");
    }
    for files in [
        json!({ "mcp/Worker.exe": "a".repeat(64), "mcp/worker.exe": "b".repeat(64) }),
        json!({ "mcp": "a".repeat(64), "mcp/worker.exe": "b".repeat(64) }),
        json!({}),
    ] {
        let mut manifest = with_component();
        manifest["components"]["mcp_windows_x86_64"]["files"] = files;
        assert!(parse(manifest).is_err());
    }
    for (field, value) in [
        ("protocol", json!(0)),
        ("bytes", json!(MAX_INSTALLER_BYTES + 1)),
        ("asset", json!("https://example.test/tool.zip")),
        ("sha256", json!("bad")),
    ] {
        let mut manifest = with_component();
        manifest["components"]["mcp_windows_x86_64"][field] = value;
        assert!(parse(manifest).is_err());
    }
}

#[test]
fn duplicate_map_keys_are_not_silently_overwritten() {
    let text = serde_json::to_string(&with_component()).unwrap();
    let duplicate = text.replace(
        "\"files\":{",
        &format!(
            "\"files\":{{\"mcp/slimeybeats-mcp.exe\":\"{}\",",
            "a".repeat(64)
        ),
    );
    assert!(UpdateManifest::parse(duplicate.as_bytes()).is_err());
    let duplicate = text.replace(
        "\"components\":{",
        "\"components\":{\"mcp_windows_x86_64\":{},",
    );
    assert!(UpdateManifest::parse(duplicate.as_bytes()).is_err());
}
