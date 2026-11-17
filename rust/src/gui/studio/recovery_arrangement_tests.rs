use super::*;
use crate::{
    gui::{recovery_capture, recovery_key, recovery_restore, test_fixture},
    studio::arrangement::{Edit, EqScope, FrameRange, ScopedEq},
};
use eframe::egui;

fn fixture(ctx: &egui::Context) -> crate::gui::app::MasterCraftApp {
    let mut app = test_fixture::app(ctx);
    let mono = Arc::new(
        Audio::new(
            (0..8000)
                .map(|frame| {
                    if frame == 0 {
                        -0.0
                    } else {
                        (frame as f32 * 0.17).sin() * 0.03
                    }
                })
                .collect(),
            8000,
            1,
        )
        .unwrap(),
    );
    let stereo = Arc::new(Audio::new([0.01, -0.02].repeat(12000), 8000, 2).unwrap());
    let editor = &mut app.studio.arrangement.editor;
    editor
        .add_sources_with_controls(vec![
            ("Short original".into(), mono.clone(), Default::default()),
            ("Long original".into(), stereo, Default::default()),
            ("Shared original".into(), mono, Default::default()),
        ])
        .unwrap();
    let origin = editor.history.as_ref().unwrap().current().clone();
    let clip = origin.lanes[0].clips[0].id.clone();
    let effect = ScopedEq {
        id: "section_eq".into(),
        scope: EqScope::Clip {
            clip_id: clip.clone(),
        },
        range: Some(FrameRange {
            start_frame: 800,
            end_frame: 1600,
        }),
        bands: crate::config::default_eq(),
        output_gain_db: -0.5,
        bypass: false,
    };
    editor.apply(Edit::SetEq { effect }).unwrap();
    editor
        .apply(Edit::MoveClip {
            clip_id: clip,
            lane_id: origin.lanes[0].id.clone(),
            start_frame: 400,
        })
        .unwrap();
    editor
        .apply(Edit::SetLane {
            lane_id: origin.lanes[0].id.clone(),
            name: None,
            gain_db: Some(-4.),
            pan: None,
            mute: None,
            solo: None,
        })
        .unwrap();
    editor.history_step(false).unwrap();
    app.studio.arrangement.options.protect_peaks = false;
    app.studio.arrangement.options.output_gain_db = -0.25;
    let reference = Arc::new(
        crate::studio::arrangement::render(
            &origin,
            &app.studio.arrangement.editor.assets,
            &app.studio.arrangement.options,
            &|| false,
        )
        .unwrap(),
    );
    app.studio.arrangement.origin = Some(origin);
    app.studio.arrangement.reference = Some(reference.clone());
    app.studio.arrangement.position = 0.08;
    app.studio.arrangement.profile = "neutral".into();
    let editor = &app.studio.arrangement.editor;
    let player = ArrangementPlayer::new(
        editor.history.as_ref().unwrap().current(),
        &editor.assets,
        &app.studio.arrangement.options,
    )
    .unwrap();
    app.studio
        .player
        .load_arrangement(reference, player)
        .unwrap();
    app.studio
        .player
        .controls
        .position
        .store(640, Ordering::Relaxed);
    app.studio.page = Page::Tracks;
    app.tab = 3;
    app
}

#[test]
fn committed_arrangement_roundtrip_restores_history_and_shared_attachments_paused() {
    let ctx = egui::Context::default();
    let mut app = fixture(&ctx);
    let editor = &mut app.studio.arrangement.editor;
    let expected = serde_json::to_value(editor.history.as_ref().unwrap().snapshot()).unwrap();
    let clip = editor.history.as_ref().unwrap().current().lanes[0].clips[0].clone();
    let lane = editor.history.as_ref().unwrap().current().lanes[0]
        .id
        .clone();
    editor
        .preview_edit(Edit::MoveClip {
            clip_id: clip.id.clone(),
            lane_id: lane,
            start_frame: 9000,
        })
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let recovery_root = root.path().join("recovery");
    let export = root.path().join("never-exported");
    app.studio.output_dir = export.clone();
    app.studio.source = Some(super::super::super::state::LoadedSource {
        audio: editor.assets[&clip.source_id].clone(),
        path: root.path().join("missing-original.wav"),
        loudness: -20.,
    });
    app.studio
        .player
        .controls
        .playing
        .store(true, Ordering::Relaxed);
    let saved = recovery_capture::capture(&app).unwrap();
    assert_eq!(
        saved.audio.len(),
        3,
        "Aliased sources must share an attachment"
    );
    assert_eq!(saved.payload["studio"]["arrangement"]["history"], expected);
    let mut store = crate::recovery_store::Store::open(
        &recovery_root,
        crate::recovery_store::DEFAULT_BUDGET_BYTES,
        &|| false,
    )
    .unwrap();
    let point = store
        .save(
            &crate::recovery_store::Snapshot {
                payload: saved.payload,
                audio: saved.audio,
            },
            &|| false,
        )
        .unwrap();
    store.finish(&|| false).unwrap();
    drop(store);
    let loaded = crate::recovery_store::load(&recovery_root, &point.checkpoint, &|| false).unwrap();
    let prepared = recovery_restore::prepare(loaded.payload, loaded.audio).unwrap();
    let mut target = test_fixture::app(&ctx);
    prepared.adopt(&mut target);
    assert_eq!(target.studio.page, Page::Tracks);
    assert!(
        target.has_recovery_work(),
        "Arrangement-only editing must be recoverable"
    );
    let editor = &target.studio.arrangement.editor;
    assert_eq!(
        serde_json::to_value(editor.history.as_ref().unwrap().snapshot()).unwrap(),
        expected
    );
    assert!(editor.draft.is_none());
    assert_eq!(
        editor
            .document()
            .unwrap()
            .clip(&clip.id)
            .unwrap()
            .start_frame,
        400
    );
    let origin = app.studio.arrangement.origin.as_ref().unwrap();
    let first = &origin.lanes[0].clips[0].source_id;
    let alias = &origin.lanes[2].clips[0].source_id;
    assert!(Arc::ptr_eq(&editor.assets[first], &editor.assets[alias]));
    assert!(Arc::ptr_eq(
        &editor.assets[first],
        &target.studio.source.as_ref().unwrap().audio
    ));
    assert_eq!(
        editor.assets[first].samples[0].to_bits(),
        (-0.0_f32).to_bits()
    );
    assert_eq!(target.studio.arrangement.options.output_gain_db, -0.25);
    assert!(target.studio.arrangement.origin.as_ref() == Some(origin));
    assert!(target.studio.player.has_live_arrangement());
    assert!(
        !target
            .studio
            .player
            .controls
            .playing
            .load(Ordering::Relaxed)
    );
    assert!(target.studio.auto_action.is_none() && target.studio.plugin_host.is_none());
    assert!(!target.studio.busy() && !export.exists());
    target.studio.arrangement.editor.history_step(true).unwrap();
    assert_eq!(
        target.studio.arrangement.editor.document().unwrap().lanes[0].gain_db,
        -4.
    );
}

#[test]
fn autosave_tracks_committed_edits_but_ignores_gestures_and_playback() {
    let ctx = egui::Context::default();
    let mut app = fixture(&ctx);
    let original = recovery_key::key(&app).unwrap();
    let editor = &mut app.studio.arrangement.editor;
    let clip = editor.history.as_ref().unwrap().current().lanes[0].clips[0].clone();
    let lane = editor.history.as_ref().unwrap().current().lanes[0]
        .id
        .clone();
    editor
        .preview_edit(Edit::MoveClip {
            clip_id: clip.id,
            lane_id: lane,
            start_frame: 1600,
        })
        .unwrap();
    app.studio
        .player
        .controls
        .position
        .store(3200, Ordering::Relaxed);
    assert_eq!(recovery_key::key(&app).unwrap(), original);
    app.studio.arrangement.editor.commit_draft().unwrap();
    let committed = recovery_key::key(&app).unwrap();
    assert_ne!(committed, original);
    app.studio.arrangement.editor.history_step(false).unwrap();
    assert_ne!(recovery_key::key(&app).unwrap(), committed);
    let saved = recovery_capture::capture(&app).unwrap();
    let mut missing = saved.payload.clone();
    let sources = missing["studio"]["arrangement"]["assets"]
        .as_object_mut()
        .unwrap();
    *sources.values_mut().next().unwrap() = serde_json::json!(999999);
    assert!(recovery_restore::prepare(missing, saved.audio.clone()).is_err());
    assert!(recovery_restore::prepare_cancellable(saved.payload, saved.audio, &|| true).is_err());
    assert_ne!(recovery_key::key(&app).unwrap(), committed);
}

#[test]
fn legacy_payload_without_arrangement_clears_a_previous_timeline() {
    let ctx = egui::Context::default();
    let mut empty = test_fixture::app(&ctx);
    empty.studio.page = Page::Stems;
    let mut saved = recovery_capture::capture(&empty).unwrap();
    saved.payload["studio"]
        .as_object_mut()
        .unwrap()
        .remove("arrangement");
    let mut target = fixture(&ctx);
    recovery_restore::prepare(saved.payload, saved.audio)
        .unwrap()
        .adopt(&mut target);
    assert_eq!(target.studio.page, Page::Stems);
    assert!(!target.studio.has_recovery_arrangement());
    assert!(!target.studio.player.has_live_arrangement());
    assert!(
        !target
            .studio
            .player
            .controls
            .playing
            .load(Ordering::Relaxed)
    );
}

#[test]
fn invalid_arrangement_relationships_are_rejected_without_replacing_open_work() {
    let ctx = egui::Context::default();
    let app = fixture(&ctx);
    let saved = recovery_capture::capture(&app).unwrap();
    let key = recovery_key::key(&app).unwrap();
    for (pointer, value) in [
        ("/studio/arrangement/version", serde_json::json!(2)),
        ("/studio/arrangement/history/capacity", serde_json::json!(0)),
        (
            "/studio/arrangement/history_budget_bytes",
            serde_json::json!(0),
        ),
        (
            "/studio/arrangement/history/current/lanes/0/clips/0/end_frame",
            serde_json::json!(999999),
        ),
        (
            "/studio/arrangement/history/redo/0/lanes/0/clips/0/source_id",
            serde_json::json!("missing"),
        ),
        (
            "/studio/arrangement/origin/lanes/0/clips/0/source_id",
            serde_json::json!("missing"),
        ),
        (
            "/studio/arrangement/options/output_gain_db",
            serde_json::json!(999999),
        ),
        ("/studio/arrangement/reference", serde_json::json!(0)),
    ] {
        let mut bad = saved.payload.clone();
        *bad.pointer_mut(pointer).expect(pointer) = value;
        assert!(
            recovery_restore::prepare(bad, saved.audio.clone()).is_err(),
            "Admitted {pointer}"
        );
        assert_eq!(recovery_key::key(&app).unwrap(), key);
    }
    let mut corrupt_audio = saved.audio.clone();
    let mut invalid = (*corrupt_audio[0]).clone();
    invalid.samples[0] = f32::NAN;
    corrupt_audio[0] = Arc::new(invalid);
    assert!(recovery_restore::prepare(saved.payload, corrupt_audio).is_err());
    assert_eq!(recovery_key::key(&app).unwrap(), key);
}

#[test]
fn inactive_arrangement_survives_restoring_another_studio_page() {
    let ctx = egui::Context::default();
    let mut app = fixture(&ctx);
    app.studio.page = Page::Voice;
    let saved = recovery_capture::capture(&app).unwrap();
    let expected = saved.payload["studio"]["arrangement"]["history"].clone();
    let mut target = fixture(&ctx);
    recovery_restore::prepare(saved.payload, saved.audio)
        .unwrap()
        .adopt(&mut target);
    assert_eq!(target.studio.page, Page::Voice);
    assert!(target.studio.has_recovery_arrangement());
    assert_eq!(
        serde_json::to_value(
            target
                .studio
                .arrangement
                .editor
                .history
                .as_ref()
                .unwrap()
                .snapshot()
        )
        .unwrap(),
        expected
    );
    assert!(!target.studio.player.has_live_arrangement());
    assert!(
        !target
            .studio
            .player
            .controls
            .playing
            .load(Ordering::Relaxed)
    );
    assert!(target.studio.arrangement.sent_epoch.is_none());
}

#[test]
fn empty_committed_timeline_preserves_undo_and_never_resumes_old_playback() {
    let ctx = egui::Context::default();
    let mut app = fixture(&ctx);
    let lanes: Vec<_> = app
        .studio
        .arrangement
        .editor
        .document()
        .unwrap()
        .lanes
        .iter()
        .map(|lane| lane.id.clone())
        .collect();
    for lane_id in lanes {
        app.studio
            .arrangement
            .editor
            .apply(Edit::RemoveLane { lane_id })
            .unwrap();
    }
    let saved = recovery_capture::capture(&app).unwrap();
    let mut target = fixture(&ctx);
    recovery_restore::prepare(saved.payload, saved.audio)
        .unwrap()
        .adopt(&mut target);
    assert_eq!(target.studio.page, Page::Tracks);
    assert_eq!(
        target
            .studio
            .arrangement
            .editor
            .document()
            .unwrap()
            .duration_frames(),
        0
    );
    assert!(
        target
            .studio
            .arrangement
            .editor
            .history
            .as_ref()
            .unwrap()
            .can_undo()
    );
    assert_eq!(target.studio.player.duration, 0.);
    assert!(
        !target
            .studio
            .player
            .controls
            .playing
            .load(Ordering::Relaxed)
    );
    assert!(target.studio.auto_action.is_none());
    target
        .studio
        .arrangement
        .editor
        .history_step(false)
        .unwrap();
    assert!(
        target
            .studio
            .arrangement
            .editor
            .document()
            .unwrap()
            .duration_frames()
            > 0
    );
}

#[test]
fn absent_reference_restores_metadata_without_substituting_other_audio_or_rendering() {
    let ctx = egui::Context::default();
    let mut app = fixture(&ctx);
    app.studio.arrangement.reference = None;
    let saved = recovery_capture::capture(&app).unwrap();
    let mut target = fixture(&ctx);
    recovery_restore::prepare(saved.payload, saved.audio)
        .unwrap()
        .adopt(&mut target);
    assert_eq!(target.studio.page, Page::Tracks);
    assert!(target.studio.has_recovery_arrangement());
    assert!(target.studio.arrangement.reference.is_none());
    assert!(target.studio.arrangement.origin.is_some());
    assert!(!target.studio.player.has_live_arrangement());
    assert!(!target.studio.busy());
    assert!(
        !target
            .studio
            .player
            .controls
            .playing
            .load(Ordering::Relaxed)
    );
    assert!(target.studio.plugin_host.is_none());
}

#[test]
fn cancelled_worker_preparation_and_page_changes_have_separate_lifecycles() {
    let ctx = egui::Context::default();
    let mut app = fixture(&ctx);
    let key = recovery_key::key(&app).unwrap();
    let saved = recovery_capture::capture(&app).unwrap();
    let polls = std::cell::Cell::new(0);
    let cancel = || {
        polls.set(polls.get() + 1);
        polls.get() >= 3
    };
    assert!(recovery_restore::prepare_cancellable(saved.payload, saved.audio, &cancel).is_err());
    assert!(polls.get() >= 3);
    assert_eq!(recovery_key::key(&app).unwrap(), key);
    app.studio.page = Page::Repair;
    assert_ne!(recovery_key::key(&app).unwrap(), key);
}
