use super::*;
use crate::app_update::{
    BuildIdentity,
    identity::{BUILD_REVISION, CHANNEL},
    manifest::is_newer,
};

fn identity(version: &str, revision: u64) -> BuildIdentity {
    BuildIdentity {
        version: version.into(),
        build_revision: revision,
        channel: CHANNEL.into(),
    }
}

#[test]
fn running_identity_preserves_the_same_version_and_explicit_revision() {
    assert_eq!(
        super::super::current(),
        identity("0.5.0-beta.3", 2026111402)
    );
}

#[test]
fn same_version_newer_revision_produces_a_candidate_with_handoff_metadata() {
    let manifest = manifest_fixture();
    let candidate = crate::app_update::fixture_candidate(
        BUILD_REVISION + 1,
        manifest["platforms"]["windows_x86_64"]["installer_sha256"]
            .as_str()
            .unwrap(),
    );
    assert_eq!(candidate.version(), "0.5.0-beta.3");
    assert_eq!(candidate.build_revision(), BUILD_REVISION + 1);
    assert_eq!(candidate.platform(), WINDOWS_X86_64);
    assert_eq!(candidate.channel(), CHANNEL);
    assert_eq!(candidate.installer_bytes(), 3);
    assert_eq!(candidate.executable_sha256(), "a".repeat(64));
    assert!(candidate.notes().contains("Fixture only"));
    assert!(
        candidate
            .release_url()
            .ends_with("/releases/tag/v0.5.0-beta.3")
    );
}

#[test]
fn exactly_current_build_is_up_to_date() {
    let mut manifest = manifest_fixture();
    manifest["build_revision"] = json!(BUILD_REVISION);
    let fixture = Fixture::new(manifest);
    let result = check_with(
        &super::super::current(),
        WINDOWS_X86_64,
        cancel(),
        &|_| {},
        &|url, max, timeout, token| fixture.read(url, max, timeout, token),
    )
    .unwrap();
    assert!(
        matches!(result, CheckResult::UpToDate { latest } if latest == super::super::current())
    );
}

#[test]
fn semver_precedence_is_numeric_and_ignores_build_metadata() {
    assert!(is_newer(&identity("0.5.0-beta.10", 2), &identity("0.5.0-beta.9", 1)).unwrap());
    assert!(is_newer(&identity("0.5.0", 2), &identity("0.5.0-rc.1", 1)).unwrap());
    assert!(is_newer(&identity("0.10.0", 2), &identity("0.9.0", 1)).unwrap());
    assert!(!is_newer(&identity("0.5.0+abc", 1), &identity("0.5.0+xyz", 1)).unwrap());
    assert!(is_newer(&identity("0.5.0+abc", 2), &identity("0.5.0+xyz", 1)).unwrap());
}

#[test]
fn neither_version_nor_revision_can_go_backwards() {
    for (version, revision) in [
        ("0.4.9", 99),
        ("0.5.0-beta.3", 1),
        ("0.5.1", 1),
        ("0.5.1", 2),
    ] {
        assert!(
            is_newer(&identity(version, revision), &identity("0.5.0-beta.3", 2)).is_err(),
            "{version}/{revision}"
        );
    }
    assert!(is_newer(&identity("0.5.0-beta.9", 99), &identity("0.5.0-beta.10", 1)).is_err());
}

#[test]
fn downgrade_metadata_cannot_be_offered_for_download() {
    for (version, revision) in [
        ("0.4.9", BUILD_REVISION + 1),
        ("0.5.0-beta.3", BUILD_REVISION - 1),
    ] {
        let mut manifest = manifest_fixture();
        manifest["version"] = json!(version);
        manifest["build_revision"] = json!(revision);
        manifest["platforms"]["windows_x86_64"]["installer_asset"] =
            json!(format!("SlimeyBeats-EZ-Master-{version}-Hotfix-Setup.exe"));
        let fixture = Fixture::new(manifest);
        let error = check_with(
            &super::super::current(),
            WINDOWS_X86_64,
            cancel(),
            &|_| {},
            &|url, max, timeout, token| fixture.read(url, max, timeout, token),
        )
        .unwrap_err();
        assert!(error.to_string().contains("refusing a downgrade"));
    }
}
