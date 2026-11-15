use super::*;
use crate::region_edit_test_support::Workspace;
use serde_json::json;
use std::sync::Arc;

fn request(edit: Value) -> Value {
    json!({"stem_set_id":"stems-baseline", "kind":"part_64", "edit":edit})
}

#[test]
fn request_schema_reuses_strict_numeric_seconds_tagged_operations_and_stable_ids() {
    for operation in [
        json!({"kind":"silence"}),
        json!({"kind":"fade_in"}),
        json!({"kind":"fade_out"}),
        json!({"kind":"gain", "db":-3.}),
        json!({"kind":"reverse"}),
    ] {
        serde_json::from_value::<EditStemRegion>(request(json!({
            "start_seconds":0.05,"end_seconds":0.15,"operation":operation
        })))
        .unwrap();
    }
    for edit in [
        json!({"start_seconds":"0.05","end_seconds":0.15,"operation":{"kind":"silence"}}),
        json!({"start_seconds":0.05,"end_seconds":null,"operation":{"kind":"silence"}}),
        json!({"start_seconds":true,"end_seconds":0.15,"operation":{"kind":"silence"}}),
        json!({"start_seconds":0.05,"end_seconds":0.15,"operation":{"kind":"split"}}),
        json!({"start_seconds":0.05,"end_seconds":0.15,"operation":{"kind":"gain"}}),
        json!({"start_seconds":0.05,"end_seconds":0.15,"operation":{"kind":"gain","db":"-3"}}),
        json!({"start_seconds":0.05,"end_seconds":0.15,"operation":{"kind":"silence","db":-3}}),
        json!({"start_seconds":0.05,"end_seconds":0.15,"operation":{"kind":"reverse"},"loop":true}),
    ] {
        assert!(serde_json::from_value::<EditStemRegion>(request(edit)).is_err());
    }
    let valid =
        request(json!({"start_seconds":0.05,"end_seconds":0.15,"operation":{"kind":"silence"}}));
    let mut extra = valid.clone();
    extra["audio_id"] = json!("guessed");
    assert!(serde_json::from_value::<EditStemRegion>(extra).is_err());
    for kind in ["Backing vocal take", "part_064", "stems-baseline:part_64"] {
        let mut invalid = valid.clone();
        invalid["kind"] = json!(kind);
        assert!(serde_json::from_value::<EditStemRegion>(invalid).is_err());
    }
    let schema = serde_json::to_value(schemars::schema_for!(EditStemRegion)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["$defs"]["RegionEdit"]["additionalProperties"], false);
    assert_eq!(
        schema["$defs"]["RegionEdit"]["properties"]["start_seconds"]["type"],
        "number"
    );
    assert_eq!(
        schema["$defs"]["RegionEdit"]["properties"]["end_seconds"]["type"],
        "number"
    );
    assert_eq!(
        schema["$defs"]["RegionOperation"]["oneOf"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert!(schema["$defs"]["StemKind"]["pattern"].is_string());
}

#[test]
fn invalid_selection_stale_bank_or_missing_kind_reserves_no_job_and_changes_nothing() {
    let workspace = Workspace::new();
    let companion = Companion::new(workspace.engine.clone());
    let before = workspace.engine.session().unwrap();
    let mut requests = vec![];
    for (start, end) in [
        (-0.1, 0.1),
        (0.1, 0.1),
        (0.15, 0.05),
        (0., 0.3),
        (0., 0.000001),
    ] {
        requests.push(request(
            json!({"start_seconds":start,"end_seconds":end,"operation":{"kind":"silence"}}),
        ));
    }
    for db in [-60.1, 12.1] {
        requests.push(request(
            json!({"start_seconds":0.05,"end_seconds":0.15,"operation":{"kind":"gain","db":db}}),
        ));
    }
    let valid =
        request(json!({"start_seconds":0.05,"end_seconds":0.15,"operation":{"kind":"silence"}}));
    let mut stale = valid.clone();
    stale["stem_set_id"] = json!("stems-stale");
    requests.push(stale);
    let mut missing = valid;
    missing["kind"] = json!("part_65");
    requests.push(missing);
    for value in requests {
        let parsed = serde_json::from_value(value).unwrap();
        assert!(companion.edit_stem_region(Parameters(parsed)).is_err());
        assert_eq!(workspace.engine.session().unwrap(), before);
        let store = workspace.engine.lock().unwrap();
        assert!(store.jobs.is_empty() && store.active.is_none());
        assert!(Arc::ptr_eq(
            store.session.studio.bank.as_ref().unwrap(),
            &workspace.bank
        ));
    }
}

#[test]
fn region_edit_waits_for_pending_control_job_and_preserves_current_bank() {
    let workspace = Workspace::new();
    let companion = Companion::new(workspace.engine.clone());
    let before = workspace.engine.session().unwrap();
    let (release, pending) = std::sync::mpsc::channel();
    workspace
        .engine
        .start(
            "mix_stems",
            |_| Ok(()),
            move |_| {
                pending.recv_timeout(std::time::Duration::from_secs(5))?;
                Ok(crate::jobs::Product::value(json!({})))
            },
        )
        .unwrap();
    let value =
        request(json!({"start_seconds":0.05,"end_seconds":0.15,"operation":{"kind":"silence"}}));
    let response = companion.edit_stem_region(Parameters(serde_json::from_value(value).unwrap()));
    release.send(()).unwrap();
    assert!(
        response
            .err()
            .unwrap()
            .contains("Another audio job is active")
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while workspace.engine.lock().unwrap().active.is_some() && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(workspace.engine.session().unwrap(), before);
    assert_eq!(workspace.engine.lock().unwrap().jobs.len(), 1);
}
