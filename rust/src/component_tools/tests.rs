use super::{
    ToolKind,
    cache::{self, Receipt},
};
use crate::source_identity::SourceIdentity;
use std::collections::BTreeMap;

#[test]
fn helper_probe_rejects_wrong_program_even_when_it_returns_successful_text() {
    assert!(
        super::install::verify_version(ToolKind::PluginHost, b"slimeybeats-vst3-host 0.1.0\n")
            .is_ok()
    );
    assert!(
        super::install::verify_version(ToolKind::McpBridge, b"slimeybeats-mcp 0.2.0-beta.3\n")
            .is_ok()
    );
    assert!(
        super::install::verify_version(ToolKind::McpBridge, b"slimeybeats-vst3-host 0.1.0\n")
            .is_err()
    );
    assert!(
        super::install::verify_version(ToolKind::McpBridge, b"slimeybeats-mcp invalid\n").is_err()
    );
}

#[test]
fn unknown_cache_material_is_not_adopted_or_removed() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path().join(ToolKind::PluginHost.key());
    std::fs::create_dir(&base).unwrap();
    let unknown = base.join("my-own-file.txt");
    std::fs::write(&unknown, b"keep me").unwrap();
    assert!(cache::owned(root.path(), ToolKind::PluginHost).is_err());
    assert_eq!(std::fs::read(unknown).unwrap(), b"keep me");
}
#[test]
fn owned_helper_receipt_round_trip_and_corruption_detection_preserve_previous_files() {
    let root = tempfile::tempdir().unwrap();
    let kind = ToolKind::PluginHost;
    let base = cache::owned(root.path(), kind).unwrap();
    let ready = base.join("ready-fixture");
    std::fs::create_dir(&ready).unwrap();
    let binary = ready.join(kind.executable());
    std::fs::write(&binary, b"fixture executable").unwrap();
    let sha = SourceIdentity::read(&binary, &|| false).unwrap().sha256;
    let receipt = Receipt {
        owner: "SlimeyBeats-EZ-Master".into(),
        tool: kind.key().into(),
        protocol: 1,
        directory: "ready-fixture".into(),
        files: BTreeMap::from([(kind.executable().into(), sha)]),
    };
    let bytes = serde_json::to_vec(&receipt).unwrap();
    std::fs::write(base.join("current.json"), &bytes).unwrap();
    assert_eq!(
        cache::ready(root.path(), kind).unwrap(),
        Some(binary.clone())
    );
    std::fs::write(&binary, b"corrupt").unwrap();
    assert!(cache::ready(root.path(), kind).is_err());
    assert_eq!(std::fs::read(base.join("current.json")).unwrap(), bytes);
    assert_eq!(std::fs::read(binary).unwrap(), b"corrupt");
}
#[test]
fn helper_receipt_cannot_point_outside_cache_or_change_protocol() {
    let root = tempfile::tempdir().unwrap();
    let kind = ToolKind::McpBridge;
    let base = cache::owned(root.path(), kind).unwrap();
    let outside = root.path().join("unrelated.exe");
    std::fs::write(&outside, b"original").unwrap();
    for (directory, protocol) in [("..", 1), ("../another", 1), ("ready-fixture", 2)] {
        let receipt = Receipt {
            owner: "SlimeyBeats-EZ-Master".into(),
            tool: kind.key().into(),
            protocol,
            directory: directory.into(),
            files: BTreeMap::from([(kind.executable().into(), "a".repeat(64))]),
        };
        std::fs::write(
            base.join("current.json"),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        assert!(cache::ready(root.path(), kind).is_err());
        assert_eq!(std::fs::read(&outside).unwrap(), b"original");
    }
}
