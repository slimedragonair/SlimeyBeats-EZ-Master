use super::*;
use crate::{doctor_help, region_edit_test_support::Workspace};
use serde_json::json;

fn arguments() -> Value {
    json!({"audio_id":"stems-baseline:part_64","repair":{"action":"reduce_low_mids","start_seconds":0.05,"end_seconds":0.15,"strength":0.5}})
}

#[test]
fn strict_shared_schema_and_guidance_explain_actions_units_and_direct_stem_workflow() {
    serde_json::from_value::<PreviewStemDoctor>(arguments()).unwrap();
    for (field, value) in [
        ("action", json!("remove_silence")),
        ("start_seconds", json!("0.05")),
        ("end_seconds", json!(false)),
        ("strength", json!("gentle")),
        ("auto_export", json!(true)),
    ] {
        let mut invalid = arguments();
        invalid["repair"][field] = value;
        assert!(serde_json::from_value::<PreviewStemDoctor>(invalid).is_err());
    }
    let mut extra = arguments();
    extra["export"] = json!(true);
    assert!(serde_json::from_value::<PreviewStemDoctor>(extra).is_err());
    let schema = serde_json::to_value(schemars::schema_for!(PreviewStemDoctor)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["$defs"]["RepairRequest"]["additionalProperties"],
        false
    );
    let help = doctor_help::get();
    let rendered = help.to_string();
    for expected in [
        "scan_audio_issues",
        "preview_stem_doctor",
        "apply_preview_to_stem",
        "undo_studio_edit",
        "0..1",
        "truncated",
    ] {
        assert!(rendered.contains(expected), "Missing {expected}");
    }
}

#[test]
fn invalid_or_stale_repairs_fail_before_job_reservation() {
    let workspace = Workspace::new();
    let companion = Companion::new(workspace.engine.clone());
    let before = workspace.engine.session().unwrap();
    for (field, value) in [
        ("start_seconds", json!(-0.1)),
        ("end_seconds", json!(0.01)),
        ("end_seconds", json!(0.3)),
        ("end_seconds", json!(0.05)),
        ("strength", json!(-0.1)),
        ("strength", json!(1.1)),
    ] {
        let mut invalid = arguments();
        invalid["repair"][field] = value;
        let parsed = serde_json::from_value(invalid).unwrap();
        assert!(companion.preview_stem_doctor(Parameters(parsed)).is_err());
    }
    for audio_id in [
        "stems-stale:part_64",
        "stems-baseline:part_65",
        "studio-stale",
    ] {
        assert!(
            companion
                .scan_audio_issues(Parameters(ScanAudioIssues {
                    audio_id: audio_id.into()
                }))
                .is_err()
        );
        let mut invalid = arguments();
        invalid["audio_id"] = json!(audio_id);
        assert!(
            companion
                .preview_stem_doctor(Parameters(serde_json::from_value(invalid).unwrap()))
                .is_err()
        );
    }
    assert_eq!(workspace.engine.session().unwrap(), before);
    let store = workspace.engine.lock().unwrap();
    assert!(store.jobs.is_empty() && store.active.is_none());
}
