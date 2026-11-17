//! Real project files and asynchronous protocol jobs; no desktop device is opened.
use super::*;
use crate::{
    arguments::Arguments, arrangement_commands as commands, arrangement_project_files as files,
    arrangement_project_operations as operations, jobs::ContextData,
    region_edit_test_support::Workspace, state::Engine,
};
use mastercraft::studio::arrangement_session::ArrangementSession;
use serde_json::json;
use std::{
    sync::{Arc, atomic::AtomicBool, mpsc},
    time::{Duration, Instant},
};

fn imported(workspace: &Workspace) -> Arc<ArrangementSession> {
    let state = commands::create(
        &workspace.engine,
        serde_json::from_value(json!({"sample_rate":8000,"channels":2})).unwrap(),
    )
    .unwrap();
    let state = commands::add(&workspace.engine, serde_json::from_value(json!({"session_id":state["session_id"],"expected_revision":0,
        "sources":[{"audio_id":"stems-baseline:vocals","name":"Voice"},{"audio_id":"stems-baseline:part_64","name":"Backing"}]})).unwrap()).unwrap();
    let clip = &state["arrangement"]["lanes"][0]["clips"][0]["id"];
    let lane = &state["arrangement"]["lanes"][0]["id"];
    let mut bands = mastercraft::config::default_eq();
    bands[6].gain = -6.;
    let state = commands::edit(&workspace.engine, serde_json::from_value(json!({"session_id":state["session_id"],"expected_revision":state["revision"],"edits":[
        {"kind":"move_clip","clip_id":clip,"lane_id":lane,"start_frame":800},
        {"kind":"set_eq","effect":{"id":"voice-section","scope":{"kind":"clip","clip_id":clip},"range":{"start_frame":300,"end_frame":800},"bands":bands}}
    ]})).unwrap()).unwrap();
    let state = commands::edit(&workspace.engine, serde_json::from_value(json!({"session_id":state["session_id"],"expected_revision":state["revision"],"edits":[{"kind":"set_lane","lane_id":lane,"gain_db":-2.}]})).unwrap()).unwrap();
    commands::history(&workspace.engine, serde_json::from_value(json!({"session_id":state["session_id"],"expected_revision":state["revision"],"redo":false})).unwrap()).unwrap();
    workspace
        .engine
        .lock()
        .unwrap()
        .arrangements
        .current
        .clone()
        .unwrap()
}

fn finished(engine: &Engine, submitted: Value) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let job = engine.job(submitted["job_id"].as_str().unwrap()).unwrap();
        if ["completed", "failed", "cancelled"].contains(&job["status"].as_str().unwrap()) {
            return job;
        }
        assert!(Instant::now() < deadline, "{job}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn save(workspace: &Workspace, session: &ArrangementSession) -> Value {
    let companion = Companion::new(workspace.engine.clone());
    let job = finished(
        &workspace.engine,
        companion
            .save_arrangement_project(Parameters(ArrangementRevision {
                session_id: session.epoch().into(),
                expected_revision: session.revision(),
            }))
            .unwrap()
            .0,
    );
    assert_eq!(job["status"], "completed", "{job}");
    job["result"].clone()
}

fn reopen(workspace: &Workspace, max_mib: u32) -> Arc<Engine> {
    Engine::new(&Arguments {
        input_root: vec![workspace.root.path().into()],
        output_root: workspace.engine.policy.output.clone(),
        max_decoded_mib: max_mib,
        decode_timeout_seconds: 5,
        desktop_exe: None,
        profiles_root: None,
        studio_data: Some(workspace.root.path().join("unused-runtime")),
        plugin_root: vec![],
        print_config: false,
    })
    .unwrap()
}

fn load_request(id: &str, session: Option<&ArrangementSession>) -> LoadArrangementProject {
    LoadArrangementProject {
        project_id: id.into(),
        replace_session_id: session.map(|s| s.epoch().into()),
        expected_revision: session.map(|s| s.revision()),
    }
}

#[test]
fn save_reopen_v2_preserves_history_scopes_source_bits_and_never_renders_a_mix() {
    let workspace = Workspace::new();
    let session = imported(&workspace);
    let legacy = workspace.engine.session().unwrap();
    let saved = save(&workspace, &session);
    let again = save(&workspace, &session);
    assert_ne!(saved["project_id"], again["project_id"]);
    assert_ne!(saved["project_path"], again["project_path"]);
    assert_eq!(workspace.engine.session().unwrap(), legacy);
    assert!(
        workspace
            .engine
            .lock()
            .unwrap()
            .arrangements
            .preview
            .is_none()
    );
    let manifest = workspace
        .engine
        .policy
        .output
        .join(saved["project_path"].as_str().unwrap());
    let disk = mastercraft::studio_native::arrangement_project::load_with_limits(
        &manifest,
        operations::limits(&workspace.engine),
        &|| false,
    )
    .unwrap();
    assert_eq!(disk.document(), session.document());
    assert_eq!(disk.document().eq.len(), 1);
    assert!(!disk.history.redo.is_empty());
    for (id, audio) in session.assets() {
        assert_eq!(
            audio
                .samples
                .iter()
                .map(|s| s.to_bits())
                .collect::<Vec<_>>(),
            disk.assets[id]
                .samples
                .iter()
                .map(|s| s.to_bits())
                .collect::<Vec<_>>()
        );
    }
    let engine = reopen(&workspace, 8);
    let old_legacy = engine.session().unwrap();
    let job = finished(
        &engine,
        Companion::new(engine.clone())
            .load_arrangement_project(Parameters(load_request(
                saved["project_id"].as_str().unwrap(),
                None,
            )))
            .unwrap()
            .0,
    );
    assert_eq!(job["status"], "completed", "{job}");
    let store = engine.lock().unwrap();
    let loaded = store.arrangements.current.as_ref().unwrap();
    assert_ne!(loaded.epoch(), session.epoch());
    assert_eq!(loaded.document(), session.document());
    assert_eq!(
        loaded.read().history.can_redo,
        session.read().history.can_redo
    );
    assert!(store.arrangements.preview.is_none());
    drop(store);
    assert_eq!(engine.session().unwrap(), old_legacy);
    assert!(
        !engine
            .cache
            .path()
            .unwrap()
            .read_dir()
            .unwrap()
            .any(|_| true)
    );
}

#[test]
fn invalid_stale_foreign_and_corrupt_requests_preserve_the_current_workspace() {
    let workspace = Workspace::new();
    let session = imported(&workspace);
    let saved = save(&workspace, &session);
    let id = saved["project_id"].as_str().unwrap();
    let companion = Companion::new(workspace.engine.clone());
    let before = commands::read(&workspace.engine).unwrap();
    assert!(
        companion
            .save_arrangement_project(Parameters(ArrangementRevision {
                session_id: session.epoch().into(),
                expected_revision: 0
            }))
            .is_err()
    );
    assert!(
        companion
            .load_arrangement_project(Parameters(load_request(id, None)))
            .is_err()
    );
    let mut stale = load_request(id, Some(&session));
    stale.expected_revision = Some(0);
    assert!(
        companion
            .load_arrangement_project(Parameters(stale))
            .is_err()
    );
    for id in [
        "../escape",
        "C:/outside/project.json",
        "arrangement-foreign000",
        "/absolute",
        "arrangement-a\\escape",
    ] {
        let job = companion.load_arrangement_project(Parameters(load_request(id, Some(&session))));
        if let Ok(value) = job {
            assert_eq!(finished(&workspace.engine, value.0)["status"], "failed");
        }
    }
    let other = Workspace::new();
    assert!(files::resolve(&other.engine, id).is_err());
    let path = workspace
        .engine
        .policy
        .output
        .join(saved["project_path"].as_str().unwrap());
    std::fs::write(&path, b"{broken").unwrap();
    let job = finished(
        &workspace.engine,
        companion
            .load_arrangement_project(Parameters(load_request(id, Some(&session))))
            .unwrap()
            .0,
    );
    assert_eq!(job["status"], "failed");
    assert!(job["error"].as_str().unwrap().contains("hash mismatch"));
    assert_eq!(commands::read(&workspace.engine).unwrap(), before);
}

#[test]
fn cancellation_and_stale_adoption_cannot_publish_a_prepared_load() {
    for cancel in [true, false] {
        let workspace = Workspace::new();
        let session = imported(&workspace);
        let saved = save(&workspace, &session);
        let request = load_request(saved["project_id"].as_str().unwrap(), Some(&session));
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let submitted = workspace
            .engine
            .start_checked(
                "held-project-load",
                |_| Ok(()),
                move |ctx| {
                    let product = operations::load(ctx, request)?;
                    ready_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(product)
                },
            )
            .unwrap();
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        if cancel {
            workspace
                .engine
                .cancel(submitted["job_id"].as_str().unwrap())
                .unwrap();
        } else {
            let mut store = workspace.engine.lock().unwrap();
            let current = store.arrangements.current.as_mut().unwrap();
            Arc::make_mut(current).undo(session.revision()).unwrap();
        }
        let before = commands::read(&workspace.engine).unwrap();
        release_tx.send(()).unwrap();
        let job = finished(&workspace.engine, submitted);
        assert_eq!(
            job["status"],
            if cancel { "cancelled" } else { "failed" },
            "{job}"
        );
        assert_eq!(commands::read(&workspace.engine).unwrap(), before);
    }
}

#[test]
fn cancelled_save_and_missing_desktop_leave_no_new_project() {
    let workspace = Workspace::new();
    let session = imported(&workspace);
    let ctx = ContextData {
        engine: workspace.engine.clone(),
        id: "cancelled-test".into(),
        snapshot: workspace.engine.lock().unwrap().session.clone(),
        cancel: Arc::new(AtomicBool::new(true)),
    };
    assert!(operations::save(ctx, session.clone(), false).is_err());
    assert!(
        Companion::new(workspace.engine.clone())
            .open_arrangement_in_desktop(Parameters(ArrangementRevision {
                session_id: session.epoch().into(),
                expected_revision: session.revision()
            }))
            .is_err()
    );
    assert!(
        !workspace
            .engine
            .policy
            .output
            .join(".slimeybeats-arrangement-projects")
            .exists()
    );
}

#[test]
fn per_input_and_aggregate_project_limits_are_enforced_before_publication() {
    let workspace = Workspace::new();
    let session = imported(&workspace);
    let snapshot = session.project_snapshot().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let limits = mastercraft::studio_native::arrangement_project::ProjectLimits {
        max_asset_bytes: 16,
        max_total_bytes: 32,
        max_output_bytes: 1024,
    };
    assert!(
        mastercraft::studio_native::arrangement_project::save_with_limits(
            &snapshot,
            dir.path(),
            limits,
            &|| false
        )
        .is_err()
    );
    assert_eq!(dir.path().read_dir().unwrap().count(), 0);
}

#[test]
fn project_schemas_reject_client_paths_and_incomplete_replacement_guards() {
    for key in ["path", "source_path", "output_path", "desktop_exe"] {
        let mut value = json!({"project_id":"arrangement-abcdef"});
        value[key] = "C:/outside".into();
        assert!(serde_json::from_value::<LoadArrangementProject>(value).is_err());
    }
    let request: LoadArrangementProject = serde_json::from_value(
        json!({"project_id":"arrangement-abcdef","replace_session_id":"arr-old"}),
    )
    .unwrap();
    assert!(operations::SelectionGuard::for_load(&request).is_err());
}

#[test]
fn asset_hash_corruption_and_changed_ownership_receipts_are_rejected() {
    let workspace = Workspace::new();
    let session = imported(&workspace);
    let saved = save(&workspace, &session);
    let id = saved["project_id"].as_str().unwrap();
    let project = files::resolve(&workspace.engine, id).unwrap();
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(&project.manifest).unwrap()).unwrap();
    let audio = project
        .manifest
        .parent()
        .unwrap()
        .join(manifest["sources"][0]["file"].as_str().unwrap());
    let mut bytes = std::fs::read(&audio).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(&audio, bytes).unwrap();
    let before = commands::read(&workspace.engine).unwrap();
    let job = finished(
        &workspace.engine,
        Companion::new(workspace.engine.clone())
            .load_arrangement_project(Parameters(load_request(id, Some(&session))))
            .unwrap()
            .0,
    );
    assert_eq!(job["status"], "failed", "{job}");
    assert_eq!(commands::read(&workspace.engine).unwrap(), before);

    let foreign = Workspace::new();
    save(&foreign, &imported(&foreign));
    let project_directory = workspace
        .engine
        .policy
        .output
        .join(".slimeybeats-arrangement-projects")
        .join(id);
    let foreign_directory = foreign
        .engine
        .policy
        .output
        .join(".slimeybeats-arrangement-projects")
        .join(id);
    std::fs::create_dir(&foreign_directory).unwrap();
    std::fs::copy(
        project_directory.join("mcp-project.json"),
        foreign_directory.join("mcp-project.json"),
    )
    .unwrap();
    assert!(
        files::resolve(&foreign.engine, id)
            .unwrap_err()
            .to_string()
            .contains("foreign")
    );
    std::fs::write(project_directory.join("mcp-project.json"), vec![b'x'; 9000]).unwrap();
    assert!(
        files::resolve(&workspace.engine, id)
            .unwrap_err()
            .to_string()
            .contains("byte budget")
    );
}

#[test]
fn failed_partial_and_stale_saves_remove_only_their_new_reservation() {
    let workspace = Workspace::new();
    let session = imported(&workspace);
    let saved = save(&workspace, &session);
    let manifest = workspace
        .engine
        .policy
        .output
        .join(saved["project_path"].as_str().unwrap());
    let original = std::fs::read(&manifest).unwrap();
    let reservation = files::Reservation::new(&workspace.engine).unwrap();
    let private = reservation.path().to_path_buf();
    let result = mastercraft::studio_native::arrangement_project::save_with_limits(
        &session.project_snapshot().unwrap(),
        reservation.path(),
        operations::limits(&workspace.engine),
        &|| {
            std::fs::read_dir(&private)
                .unwrap()
                .flatten()
                .any(|directory| {
                    std::fs::read_dir(directory.path().join("assets"))
                        .is_ok_and(|mut entries| entries.next().is_some())
                })
        },
    );
    assert!(result.is_err());
    drop(reservation);
    assert!(!private.exists());
    {
        let mut store = workspace.engine.lock().unwrap();
        Arc::make_mut(store.arrangements.current.as_mut().unwrap())
            .undo(session.revision())
            .unwrap();
    }
    let context = workspace.context("stale-save");
    assert!(operations::save(context, session, false).is_err());
    assert_eq!(std::fs::read(&manifest).unwrap(), original);
    assert_eq!(
        workspace
            .engine
            .policy
            .output
            .join(".slimeybeats-arrangement-projects")
            .read_dir()
            .unwrap()
            .filter(|entry| entry.as_ref().unwrap().file_type().unwrap().is_dir())
            .count(),
        1
    );
}

#[test]
fn reopening_enforces_the_current_hosts_per_source_budget() {
    let workspace = Workspace::new();
    let saved = save(&workspace, &imported(&workspace));
    let mut engine = reopen(&workspace, 8);
    Arc::get_mut(&mut engine).unwrap().max_decoded_bytes = 32;
    let job = finished(
        &engine,
        Companion::new(engine.clone())
            .load_arrangement_project(Parameters(load_request(
                saved["project_id"].as_str().unwrap(),
                None,
            )))
            .unwrap()
            .0,
    );
    assert_eq!(job["status"], "failed", "{job}");
    assert!(job["error"].as_str().unwrap().contains("budget"));
    assert!(engine.lock().unwrap().arrangements.current.is_none());
}

#[test]
fn cold_help_examples_match_the_real_persistence_schemas() {
    let help = crate::arrangement_project_help::get();
    for key in ["save", "desktop"] {
        serde_json::from_value::<ArrangementRevision>(help[key]["arguments"].clone()).unwrap();
    }
    for key in ["restart_reopen", "replace_open_workspace"] {
        let request: LoadArrangementProject =
            serde_json::from_value(help[key]["arguments"].clone()).unwrap();
        operations::SelectionGuard::for_load(&request).unwrap();
    }
    assert_eq!(
        crate::capabilities::describe(&Workspace::new().engine)["tool_count"],
        58
    );
    assert!(
        help["identity"]
            .as_str()
            .unwrap()
            .contains("SAME configured output root")
    );
}

#[test]
fn legacy_export_help_explicitly_separates_tracks_when_both_workspaces_exist() {
    let help = crate::studio_help::get("studio_export");
    let scope = help["scope"].as_str().unwrap();
    assert!(scope.contains("LEGACY") && scope.contains("both workspaces exist"));
    let routing = help["arrangement_workspace"].to_string();
    for tool in [
        "save_arrangement_project",
        "load_arrangement_project",
        "open_arrangement_in_desktop",
        "export_arrangement",
        "undo_arrangement",
    ] {
        assert!(routing.contains(tool), "Missing Tracks pointer: {tool}");
    }
    for example in help["projects"].as_array().unwrap() {
        assert!(example["when"].as_str().unwrap().contains("LEGACY"));
    }
}

#[cfg(unix)]
#[test]
fn linked_project_directories_receipts_and_assets_are_rejected() {
    let workspace = Workspace::new();
    let saved = save(&workspace, &imported(&workspace));
    let id = saved["project_id"].as_str().unwrap();
    let project = files::resolve(&workspace.engine, id).unwrap();
    let assets = project.manifest.parent().unwrap().join("assets");
    let retained = assets.with_file_name("retained-assets");
    std::fs::rename(&assets, &retained).unwrap();
    std::os::unix::fs::symlink(&retained, &assets).unwrap();
    assert!(
        files::resolve(&workspace.engine, id)
            .unwrap_err()
            .to_string()
            .contains("symlinks")
    );
}
