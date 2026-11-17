use super::*;
use crate::{jobs::Product, region_edit_test_support::Workspace};
use mastercraft::studio::arrangement::render;
use serde_json::json;
use std::time::{Duration, Instant};

fn imported(workspace: &Workspace) -> (Companion, Value) {
    let companion = Companion::new(workspace.engine.clone());
    let created = companion
        .create_arrangement(Parameters(
            serde_json::from_value(json!({"sample_rate":8000,"channels":2})).unwrap(),
        ))
        .unwrap()
        .0;
    let state = companion.add_arrangement_assets(Parameters(serde_json::from_value(json!({"session_id":created["session_id"],"expected_revision":0,
        "sources":[{"audio_id":"stems-baseline:vocals","name":"Repeated"},{"audio_id":"stems-baseline:part_64","name":"Repeated"}]})).unwrap())).unwrap().0;
    (companion, state)
}
fn finished(engine: &crate::state::Engine, value: Value) -> Value {
    let id = value["job_id"].as_str().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let job = engine.job(id).unwrap();
        if job["status"] == "completed" {
            return job["result"].clone();
        }
        assert!(
            job["status"] != "failed" && job["status"] != "cancelled",
            "{job}"
        );
        assert!(Instant::now() < deadline, "Job timeout: {job}");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn typed_metadata_commands_keep_sources_and_legacy_bank_and_reject_stale_batches() {
    let workspace = Workspace::new();
    let (companion, state) = imported(&workspace);
    let legacy = workspace.engine.session().unwrap();
    let lane = state["arrangement"]["lanes"][0]["id"].as_str().unwrap();
    let asset = state["arrangement"]["lanes"][0]["clips"][0]["source_id"]
        .as_str()
        .unwrap();
    let arc = workspace
        .engine
        .lock()
        .unwrap()
        .arrangements
        .current
        .as_ref()
        .unwrap()
        .assets()[asset]
        .clone();
    let request = json!({"session_id":state["session_id"],"expected_revision":state["revision"],"edits":[{"kind":"set_lane","lane_id":lane,"gain_db":-6.0}]});
    let edited = companion
        .edit_arrangement(Parameters(serde_json::from_value(request.clone()).unwrap()))
        .unwrap()
        .0;
    assert_eq!(edited["arrangement"]["lanes"][0]["gain_db"], -6.0);
    assert!(edited["history"]["can_undo"].as_bool().unwrap());
    assert!(
        companion
            .edit_arrangement(Parameters(serde_json::from_value(request).unwrap()))
            .is_err()
    );
    assert_eq!(workspace.engine.session().unwrap(), legacy);
    let store = workspace.engine.lock().unwrap();
    assert!(store.arrangements.preview.is_none() && store.jobs.is_empty());
    assert!(Arc::ptr_eq(
        &arc,
        &store.arrangements.current.as_ref().unwrap().assets()[asset]
    ));
    assert!(Arc::ptr_eq(
        &workspace.bank,
        store.session.studio.bank.as_ref().unwrap()
    ));
}
#[test]
fn real_render_job_and_float_export_preserve_core_pcm_and_clear_releases_workspace() {
    let workspace = Workspace::new();
    let (companion, state) = imported(&workspace);
    let revision: ArrangementRevision = serde_json::from_value(
        json!({"session_id":state["session_id"],"expected_revision":state["revision"]}),
    )
    .unwrap();
    let rendered = finished(
        &workspace.engine,
        companion
            .render_arrangement(Parameters(revision))
            .unwrap()
            .0,
    );
    let path = workspace
        .engine
        .policy
        .output
        .join(rendered["relative_preview_path"].as_str().unwrap());
    let samples = hound::WavReader::open(&path)
        .unwrap()
        .samples::<f32>()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let selected = workspace
        .engine
        .lock()
        .unwrap()
        .arrangements
        .current
        .clone()
        .unwrap();
    let expected = render(
        selected.document(),
        selected.assets(),
        selected.render_options(),
        &|| false,
    )
    .unwrap();
    assert_eq!(samples, expected.samples);
    let exported = companion.export_arrangement(Parameters(serde_json::from_value(json!({"session_id":state["session_id"],"expected_revision":state["revision"],"name":"arrangement-proof","format":"wav32f"})).unwrap())).unwrap().0;
    let result = finished(&workspace.engine, exported);
    let output = workspace
        .engine
        .policy
        .output
        .join(result["exports"][0]["path"].as_str().unwrap());
    let saved = hound::WavReader::open(output)
        .unwrap()
        .samples::<f32>()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(saved, expected.samples);
    workspace.engine.clear().unwrap();
    assert!(companion.get_arrangement().unwrap().0["arrangement"].is_null());
    assert!(!path.exists());
}
#[test]
fn strict_schemas_and_busy_admission_preserve_current_metadata() {
    let workspace = Workspace::new();
    let (companion, state) = imported(&workspace);
    let schema = serde_json::to_value(schemars::schema_for!(EditArrangement)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert!(
        serde_json::from_value::<EditArrangement>(
            json!({"session_id":"x","expected_revision":0,"edits":[],"path":"private.wav"})
        )
        .is_err()
    );
    let (release, pending) = std::sync::mpsc::channel();
    workspace
        .engine
        .start(
            "held-test",
            |_| Ok(()),
            move |_| {
                pending.recv_timeout(Duration::from_secs(3))?;
                Ok(Product::value(json!({})))
            },
        )
        .unwrap();
    let history = companion.undo_arrangement(Parameters(
        serde_json::from_value(
            json!({"session_id":state["session_id"],"expected_revision":state["revision"]}),
        )
        .unwrap(),
    ));
    assert!(history.is_err());
    release.send(()).unwrap();
    assert_eq!(companion.get_arrangement().unwrap().0, state);
    let help = crate::arrangement_help::get().to_string();
    for text in [
        "SOURCE frames",
        "PROJECT frames",
        "exclude_eq_id",
        "never",
        "export_arrangement",
    ] {
        assert!(help.contains(text), "Missing {text}");
    }
}

#[test]
fn fresh_discovery_advertises_arrangement_without_the_old_global_exclusion() {
    let workspace = Workspace::new();
    let companion = Companion::new(workspace.engine.clone());
    let info = rmcp::ServerHandler::get_info(&companion);
    let instructions = info.instructions.as_deref().unwrap();
    for text in [
        "create_arrangement",
        "get_arrangement",
        "get_help(topic=arrangement)",
    ] {
        assert!(instructions.contains(text), "Missing {text}");
    }
    assert!(!instructions.contains("This exposes no clip arrangement"));
    let caps = crate::capabilities::describe(&workspace.engine);
    assert!(
        caps["help_topics"]
            .as_array()
            .unwrap()
            .contains(&json!("arrangement"))
    );
    assert_eq!(caps["arrangement"]["typed_metadata_edits"], true);
}

#[test]
fn arrangement_help_has_a_typed_complete_scoped_eq_and_sequential_source_loop() {
    let help = crate::arrangement_help::get();
    let example: crate::arrangement_arguments::EditArrangement =
        serde_json::from_value(help["example_set_eq"]["arguments"].clone()).unwrap();
    let mastercraft::studio::arrangement::Edit::SetEq { effect } = &example.edits[0] else {
        panic!("Expected an actual scoped EQ edit");
    };
    assert_eq!(effect.bands.len(), 7);
    assert_eq!(effect.bands[6].freq, 10000.0);
    assert_eq!(effect.bands[6].gain, -6.0);
    assert!(effect.bands[..6].iter().all(|band| band.gain == 0.0));
    assert_eq!(effect.range.unwrap().start_frame, 48000);
    assert_eq!(effect.range.unwrap().end_frame, 96000);
    let guidance = help.to_string();
    for term in [
        "BEFORE loading the next",
        "remain owned",
        "144000",
        "192000",
        "EXACT same scope and range",
        "no master_preview",
    ] {
        assert!(guidance.contains(term), "Missing {term}");
    }
    assert!(
        crate::help::get(crate::help::Topic::Eq)["arrangement"]
            .as_str()
            .unwrap()
            .contains("edit_arrangement")
    );
}
