use super::*;
use crate::{
    arguments::Arguments,
    support_report_protocol::{PrepareSupportReport, SaveSupportReport},
};
use mastercraft::{audio::Audio, session_document::SessionSource, source_identity::SourceIdentity};
use std::{path::Path, sync::Arc};

fn engine(root: &Path) -> Arc<Engine> {
    Engine::new(&Arguments {
        input_root: vec![root.into()],
        output_root: root.join("exports"),
        max_decoded_mib: 8,
        decode_timeout_seconds: 5,
        desktop_exe: None,
        profiles_root: None,
        studio_data: Some(root.join("unused-private-cache")),
        plugin_root: vec![],
        print_config: false,
    })
    .unwrap()
}

fn request() -> PrepareSupportReport {
    serde_json::from_value(json!({"category":"audio_playback", "title":"Playback stops", "details":"Press Play, then Stop."})).unwrap()
}

fn save_request(review: &Value, filename: &str, confirmed: bool) -> SaveSupportReport {
    SaveSupportReport {
        review_id: review["review_id"].as_str().unwrap().into(),
        filename: filename.into(),
        owner_review_confirmed: confirmed,
    }
}

#[test]
fn schemas_reject_unknown_fields_and_consent_defaults_false() {
    for value in [
        json!({"logs":"private"}),
        json!({"context":{"path":"private"}}),
        json!({"category":"song-name"}),
        json!({"title":false}),
    ] {
        assert!(serde_json::from_value::<PrepareSupportReport>(value).is_err());
    }
    let defaults: PrepareSupportReport = serde_json::from_value(json!({})).unwrap();
    assert!(defaults.title.is_empty() && defaults.details.is_empty());
    let save: SaveSupportReport =
        serde_json::from_value(json!({"review_id":"report", "filename":"report.md"})).unwrap();
    assert!(!save.owner_review_confirmed);
    for value in [
        json!({"review_id":"r", "filename":"r.md", "owner_review_confirmed":"true"}),
        json!({"review_id":"r", "filename":"r.md", "url":"https://invalid.test"}),
        json!({"filename":"r.md", "owner_review_confirmed":true}),
    ] {
        assert!(serde_json::from_value::<SaveSupportReport>(value).is_err());
    }
    for schema in [
        serde_json::to_value(schemars::schema_for!(PrepareSupportReport)).unwrap(),
        serde_json::to_value(schemars::schema_for!(SaveSupportReport)).unwrap(),
    ] {
        assert_eq!(schema["additionalProperties"], false);
    }
}

#[test]
fn prepare_without_loaded_audio_does_not_create_files_or_jobs() {
    let root = tempfile::tempdir().unwrap();
    let engine = engine(root.path());
    let reviews = Mutex::default();
    let before = engine.session().unwrap();
    let entries = std::fs::read_dir(&engine.policy.output).unwrap().count();
    let result = prepare(&engine, &reviews, request()).unwrap();
    assert_eq!(result["installed_desktop_version"], Value::Null);
    let preview = result["preview_markdown"].as_str().unwrap();
    assert!(preview.contains(&format!(
        "\"companion_version\": \"{}\"",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(preview.contains("\"linked_engine_build\""));
    assert!(preview.contains("\"installed_desktop_version\": null"));
    let diagnostics = preview
        .rsplit_once("```json\n")
        .unwrap()
        .1
        .split_once("\n```")
        .unwrap()
        .0;
    let diagnostics: Value = serde_json::from_str(diagnostics).unwrap();
    assert_eq!(
        diagnostics["tools"],
        json!({
            "audio_codecs_ready": null, "stem_engine_ready": null,
            "vst3_host_ready": null, "mcp_bridge_ready": null,
        })
    );
    assert!(
        result["client_privacy_notice"]
            .as_str()
            .unwrap()
            .contains("model provider")
    );
    assert_eq!(before, engine.session().unwrap());
    assert_eq!(
        std::fs::read_dir(&engine.policy.output).unwrap().count(),
        entries
    );
    assert!(!root.path().join("unused-private-cache").exists());
}

#[test]
fn failed_prepare_preserves_current_review_and_successful_prepare_invalidates_it() {
    let root = tempfile::tempdir().unwrap();
    let engine = engine(root.path());
    let reviews = Mutex::default();
    let first = prepare(&engine, &reviews, request()).unwrap();
    let mut invalid = request();
    invalid.details = "x".repeat(support_report::MAX_DETAILS_BYTES + 1);
    assert!(prepare(&engine, &reviews, invalid).is_err());
    save(&engine, &reviews, save_request(&first, "first.md", true)).unwrap();
    assert_eq!(
        std::fs::read_to_string(engine.policy.output.join("first.md")).unwrap(),
        first["preview_markdown"]
    );
    let second = prepare(&engine, &reviews, request()).unwrap();
    assert_ne!(first["review_id"], second["review_id"]);
    assert!(save(&engine, &reviews, save_request(&first, "stale.md", true)).is_err());
    assert!(!engine.policy.output.join("stale.md").exists());
    assert_eq!(
        reviews.lock().unwrap().current.as_ref().unwrap().id,
        second["review_id"]
    );
}

#[test]
fn save_requires_explicit_review_and_rejects_foreign_ids_and_path_names() {
    let root = tempfile::tempdir().unwrap();
    let engine = engine(root.path());
    let reviews = Mutex::default();
    let prepared = prepare(&engine, &reviews, request()).unwrap();
    assert!(
        save(
            &engine,
            &reviews,
            save_request(&prepared, "report.md", false)
        )
        .is_err()
    );
    for name in [
        "../escape.md",
        "/tmp/escape.md",
        "C:\\escape.md",
        "nested/report.md",
        "nested\\report.md",
        "CON.md",
        "a:b.md",
        "a..b.md",
        ".hidden.md",
        "report.txt",
        "report.md ",
        "",
    ] {
        assert!(
            save(&engine, &reviews, save_request(&prepared, name, true)).is_err(),
            "Accepted {name}"
        );
    }
    for id in [
        "foreign-secret-token".to_string(),
        "x".repeat(129),
        String::new(),
    ] {
        let mut foreign = save_request(&prepared, "report.md", true);
        foreign.review_id = id.clone();
        let error = save(&engine, &reviews, foreign).unwrap_err().to_string();
        if !id.is_empty() {
            assert!(!error.contains(&id));
        }
    }
    let other_root = tempfile::tempdir().unwrap();
    let other_engine = self::engine(other_root.path());
    let other_reviews = Mutex::default();
    prepare(&other_engine, &other_reviews, request()).unwrap();
    assert!(
        save(
            &other_engine,
            &other_reviews,
            save_request(&prepared, "foreign.md", true)
        )
        .is_err()
    );
    assert!(
        std::fs::read_dir(&engine.policy.output)
            .unwrap()
            .all(|entry| entry.unwrap().path().is_dir())
    );
}

#[test]
fn exact_reviewed_bytes_survive_state_changes_and_output_collision() {
    let root = tempfile::tempdir().unwrap();
    let engine = engine(root.path());
    let reviews = Mutex::default();
    let report = prepare(&engine, &reviews, request()).unwrap();
    // A later audio/settings change must not silently regenerate the report.
    engine.lock().unwrap().session.settings.eq_output_db = -3.;
    let before = engine.session().unwrap();
    std::fs::write(engine.policy.output.join("report.md"), b"previous report").unwrap();
    assert!(save(&engine, &reviews, save_request(&report, "report.md", true)).is_err());
    assert_eq!(
        std::fs::read(engine.policy.output.join("report.md")).unwrap(),
        b"previous report"
    );
    let saved = save(&engine, &reviews, save_request(&report, "fresh.md", true)).unwrap();
    assert_eq!(saved["uploaded"], false);
    assert_eq!(saved["filename"], "fresh.md");
    let bytes = std::fs::read(engine.policy.output.join("fresh.md")).unwrap();
    assert_eq!(
        bytes,
        report["preview_markdown"].as_str().unwrap().as_bytes()
    );
    assert_eq!(saved["bytes"].as_u64().unwrap() as usize, bytes.len());
    assert_eq!(before, engine.session().unwrap());
}

#[test]
fn typed_context_never_reads_audio_or_collects_private_names_errors_or_projects() {
    use crate::{
        jobs::JobRecord,
        state::Input,
        studio_state::{EditedAudio, StudioOrigin},
    };
    let root = tempfile::tempdir().unwrap();
    let engine = engine(root.path());
    let audio = Arc::new(Audio::new(vec![0.125; 8000], 8000, 1).unwrap());
    let source = Arc::new(Input {
        id: "PrivateSourceName".into(),
        source: SessionSource {
            path: root.path().join("SecretUser/UnreleasedSong.wav"),
            identity: SourceIdentity {
                bytes: 32,
                sha256: "a".repeat(64),
            },
        },
        audio: audio.clone(),
        generated: None,
        studio_preview: None,
    });
    let preview = Arc::new(EditedAudio {
        id: "PrivatePreviewID".into(),
        label: "PrivateStemName".into(),
        audio: audio.clone(),
        before: audio.clone(),
        file: None,
        steps: vec![
            json!({"plugin":"PrivatePluginName", "project":"/home/SecretUser/PrivateProject", "api_key":"sk-PrivateSecret1234567890"}),
        ],
        notes: vec!["raw error\nSecretUser\nNondeterministicError019234".into()],
        origin_snapshot: StudioOrigin {
            id: "PrivateOriginID".into(),
            label: "PrivateOriginName".into(),
            audio: audio.clone(),
        },
        bank_snapshot: None,
    });
    {
        let mut state = engine.lock().unwrap();
        state.session.source = Some(source.clone());
        state.session.settings.profile = "custom:PrivateProfileName".into();
        state.session.settings.reference = Some(root.path().join("PrivateReference.wav"));
        state.session.studio.current = Some(preview.clone());
        state.jobs.insert(
            "private-job".into(),
            JobRecord {
                job_id: "private-job".into(),
                kind: "PrivateJobName".into(),
                status: "failed".into(),
                progress: 0.,
                message: "PrivateDiagnosticText".into(),
                result: None,
                error: Some("SecretUser\nraw error\nSecretToken=NOT_FOR_REPORT".into()),
                started: std::time::Instant::now(),
                elapsed_seconds: None,
            },
        );
    }
    let before = engine.session().unwrap();
    let reviews = Mutex::default();
    let report = prepare(&engine, &reviews, request()).unwrap();
    let returned = serde_json::to_string(&report).unwrap();
    for secret in [
        "SecretUser",
        "UnreleasedSong",
        "PrivateSourceName",
        "PrivatePreviewID",
        "PrivateStemName",
        "PrivatePluginName",
        "PrivateProject",
        "PrivateSecret",
        "NondeterministicError",
        "PrivateOrigin",
        "PrivateProfileName",
        "PrivateReference",
        "PrivateJobName",
        "PrivateDiagnosticText",
        "NOT_FOR_REPORT",
    ] {
        assert!(!returned.contains(secret), "Leaked {secret}");
    }
    save(&engine, &reviews, save_request(&report, "report.md", true)).unwrap();
    assert_eq!(before, engine.session().unwrap());
    let state = engine.lock().unwrap();
    assert!(Arc::ptr_eq(state.session.source.as_ref().unwrap(), &source));
    assert!(Arc::ptr_eq(
        state.session.studio.current.as_ref().unwrap(),
        &preview
    ));
    assert_eq!(audio.samples, vec![0.125; 8000]);
    assert_eq!(state.jobs.len(), 1);
    assert_eq!(state.generation, 0);
    assert!(!source.source.path.exists());
}

#[test]
fn user_text_is_filtered_and_privacy_help_is_self_contained() {
    let root = tempfile::tempdir().unwrap();
    let engine = engine(root.path());
    let mut draft = request();
    draft.details = "C:\\Users\\SecretPerson\\SecretSong.wav\nprivate.person@example.test\nAPI_KEY=NOT_FOR_REPORT".into();
    let report = prepare(&engine, &Mutex::default(), draft).unwrap();
    assert_eq!(report["text_was_changed"], true);
    for secret in [
        "SecretPerson",
        "SecretSong",
        "private.person",
        "NOT_FOR_REPORT",
    ] {
        assert!(
            !report["preview_markdown"]
                .as_str()
                .unwrap()
                .contains(secret)
        );
    }
    let guide = help().to_string();
    for term in [
        "prepare_support_report",
        "save_support_report",
        "owner_review_confirmed",
        "model provider",
        "installed_desktop_version",
    ] {
        assert!(guide.contains(term));
    }
    assert!(
        recovery_help()
            .to_string()
            .contains("no MCP recovery-cache filesystem access")
    );
}
