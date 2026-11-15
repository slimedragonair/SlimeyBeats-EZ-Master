use super::*;

#[test]
fn diagnostic_allowlist_cannot_serialize_unrelated_user_material() {
    // Representative session/log/config material must be projected to typed
    // counts/flags by the caller, never attached to a support context.
    let source = serde_json::json!({
        "tracks": [{"path": "C:\\Users\\PrivateUser\\Music\\Unreleased Song.wav"}],
        "reference": "My Secret Reference", "stem": "Unreleased Vocals",
        "plugin": "Private Plugin Name", "custom_profile": "My Private Tonal Profile",
        "api_key": "sk-FAKEPRIVATEKEY1234567890", "user": "PrivateUser",
        "error": "line one\nC:\\Users\\PrivateUser\\nondeterministic-993822.log\nsecret=DO_NOT_LEAK",
        "environment": {"HOME": "/home/PrivateUser", "TOKEN": "DO_NOT_LEAK"}
    });
    let mut context = ReportContext::current();
    context.counts.tracks = source["tracks"].as_array().unwrap().len() as u32;
    context.activity.session_loaded = true;
    let automatic = serde_json::to_value(&context).unwrap();
    let keys: std::collections::BTreeSet<_> = automatic
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        std::collections::BTreeSet::from([
            "application",
            "operating_system",
            "architecture",
            "workflow",
            "counts",
            "tools",
            "activity"
        ])
    );
    assert_eq!(
        automatic,
        serde_json::json!({
            "application": crate::app_update::current(),
            "operating_system": OperatingSystem::current(),
            "architecture": Architecture::current(),
            "workflow": "other",
            "counts": {"tracks": 1, "stems": 0, "eq_bands": 0},
            "tools": {"audio_codecs_ready": null, "stem_engine_ready": null,
                "vst3_host_ready": null, "mcp_bridge_ready": null},
            "activity": {"session_loaded": true, "preview_available": false,
                "processing_active": false, "recovery_available": false}
        })
    );
    let report = build(
        &context,
        &ReportDraft::new(IssueCategory::Crash, "", "").unwrap(),
    )
    .unwrap();
    for secret in [
        "PrivateUser",
        "Unreleased Song",
        "My Secret Reference",
        "Unreleased Vocals",
        "Private Plugin Name",
        "My Private Tonal Profile",
        "sk-FAKE",
        "nondeterministic",
        "DO_NOT_LEAK",
        "HOME",
    ] {
        assert!(
            !report.markdown().contains(secret),
            "Leaked fixture: {secret}"
        );
    }
    assert!(report.markdown().contains(REVIEW_NOTICE));
    assert!(!report.markdown().contains("log files attached"));
}

#[test]
fn obvious_windows_posix_unc_and_spaced_paths_are_removed() {
    let lines = [
        r"Open C:\Users\SecretUser\Music\Unreleased Song.wav",
        "Open /home/SecretUser/Music/Unreleased Song.wav",
        r"Open \\PrivateServer\SecretShare\Unreleased Song.wav",
        "Open //PrivateServer/SecretShare/Unreleased Song.wav",
        r#"Open "D:/SecretUser/Unreleased Song.wav" then retry"#,
        "Open '/Users/SecretUser/Unreleased Song.wav' then retry",
        r"Open \\?\C:\Users\SecretUser\Music\song.wav",
    ];
    for line in lines {
        let draft = ReportDraft::new(IssueCategory::Other, line, line).unwrap();
        assert!(draft.text_was_changed());
        for text in [draft.title(), draft.details()] {
            for secret in [
                "SecretUser",
                "Unreleased Song",
                "PrivateServer",
                "SecretShare",
            ] {
                assert!(!text.contains(secret), "Leaked {secret} from {line}");
            }
            assert!(text.contains("[path removed]"));
        }
    }
}

#[test]
fn emails_assignments_known_keys_bearer_and_token_like_values_are_removed() {
    let cases = [
        (
            "email me at private.person@example.test",
            "private.person@example.test",
        ),
        ("API_KEY = secret-even-if-short", "secret-even-if-short"),
        ("OPENAI_API_KEY=another-short-key", "another-short-key"),
        ("GITHUB_TOKEN=short-github-key", "short-github-key"),
        ("password: short-secret", "short-secret"),
        (
            "Authorization: Bearer important-sensitive-value",
            "important-sensitive-value",
        ),
        ("Bearer another-sensitive-value", "another-sensitive-value"),
        (
            "Paste sk-ExampleFixtureKey1234567890",
            "sk-ExampleFixtureKey1234567890",
        ),
        (
            "Paste ghp_ExampleFixtureKey1234567890",
            "ghp_ExampleFixtureKey1234567890",
        ),
        (
            "Paste abcABC1234567890_defGHI0123456789",
            "abcABC1234567890_defGHI0123456789",
        ),
        ("unexpected error\ntoken=FAKE_TOKEN\nretry", "FAKE_TOKEN"),
    ];
    for (input, secret) in cases {
        let draft = ReportDraft::new(IssueCategory::Other, "", input).unwrap();
        assert!(draft.text_was_changed(), "Expected filter change: {input}");
        let report = build(&ReportContext::current(), &draft).unwrap();
        assert!(!report.markdown().contains(secret), "Leaked {secret}");
    }
}

#[test]
fn private_key_blocks_control_characters_and_direction_overrides_are_removed() {
    let details = "A\0 description\n-----BEGIN PRIVATE KEY-----\nHiddenSecretEvenWithoutDigits\n-----END PRIVATE KEY-----\nC:\\Users\\HiddenUser\\song.wav\nAPI\u{200b}_KEY=HiddenCredential";
    let draft = ReportDraft::new(IssueCategory::Other, "An\nissue\u{202e}", details).unwrap();
    let report = build(&ReportContext::current(), &draft).unwrap();
    for secret in [
        "HiddenSecretEvenWithoutDigits",
        "HiddenUser",
        "HiddenCredential",
        "\0",
        "\u{202e}",
        "\u{200b}",
    ] {
        assert!(!report.markdown().contains(secret), "Leaked {secret}");
    }
    assert_eq!(draft.title(), "An issue");
}

#[test]
fn normal_prose_unicode_and_safe_counts_are_preserved() {
    let text = "Soloing a stem stops playback.\nI used 9 stems at 48 kHz.\n日本語の説明 🎵";
    let draft = ReportDraft::new(IssueCategory::AudioPlayback, "Playback issue", text).unwrap();
    assert_eq!(draft.details(), text);
    assert!(!draft.text_was_changed());
    // Relative names are not reliably distinguishable from ordinary prose. The
    // warning must remain visible rather than promising perfect name redaction.
    let typed = ReportDraft::new(IssueCategory::Other, "", "My track is named Quiet Moon").unwrap();
    assert!(typed.details().contains("Quiet Moon"));
    assert!(REVIEW_NOTICE.contains("cannot guarantee"));
}

#[test]
fn failed_saves_do_not_echo_selected_private_paths() {
    let root = tempfile::tempdir().unwrap();
    let report = build(
        &ReportContext::current(),
        &ReportDraft::new(IssueCategory::Other, "", "").unwrap(),
    )
    .unwrap();
    let error = report
        .write_new(&root.path().join("PrivateUser/PrivateSong.md"))
        .unwrap_err();
    let shown = format!("{error:#}");
    assert!(!shown.contains("PrivateUser"));
    assert!(!shown.contains("PrivateSong"));
    assert!(!shown.contains(&root.path().display().to_string()));
    assert_eq!(error.chain().count(), 1);
}
