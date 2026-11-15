use super::{
    app::MasterCraftApp, recovery_capture, recovery_key, recovery_state::RecoveryState,
    test_fixture, theme, ui_test_input::settled,
};
use crate::recovery_store::{self, Snapshot, Store};
use eframe::egui;
use std::{
    path::Path,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

fn ready(ctx: &egui::Context) -> MasterCraftApp {
    theme::apply(ctx);
    let mut app = test_fixture::mastered(ctx);
    settled(ctx, &mut app, egui::vec2(1100., 720.));
    app
}
fn pump(app: &mut MasterCraftApp, ctx: &egui::Context, done: impl Fn(&MasterCraftApp) -> bool) {
    let until = Instant::now() + Duration::from_secs(20);
    while !done(app) && Instant::now() < until {
        app.tick_recovery(ctx);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        done(app),
        "Recovery did not settle: {} / {:?}",
        app.recovery.message,
        app.recovery.error
    );
}
fn seed(root: &Path, app: &MasterCraftApp) -> recovery_store::CheckpointRef {
    let capture = recovery_capture::capture(app).unwrap();
    let mut store = Store::open(root, recovery_store::DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    store
        .save(
            &Snapshot {
                payload: capture.payload,
                audio: capture.audio,
            },
            &|| false,
        )
        .unwrap()
        .checkpoint
    // Drop, without finish: this represents an interrupted process.
}
#[test]
fn restore_saves_current_work_first_and_never_exports_or_autoplays() {
    let ctx = egui::Context::default();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Recovery");
    let mut previous = ready(&ctx);
    previous.settings.intensity = 0.83;
    let selected = seed(&root, &previous);
    let mut current = ready(&ctx);
    current.settings.intensity = 0.27;
    current.settings.out_dir = temp.path().join("Never exported");
    let current_original = current.current().unwrap().1.before.samples.clone();
    current.recovery = RecoveryState::load(Some(root.clone()), temp.path().join("prefs.json"));
    pump(&mut current, &ctx, |a| {
        a.recovery.startup_done && !a.recovery.busy()
    });
    current.request_recovery_restore(selected, &ctx);
    pump(&mut current, &ctx, |a| a.recovery.restore_completed);
    assert_eq!(current.settings.intensity, 0.83);
    assert!(!current.player.controls.playing.load(Ordering::Relaxed));
    assert!(!temp.path().join("Never exported").exists());
    let backup = recovery_store::list_history(&root, &|| false)
        .unwrap()
        .checkpoints
        .into_iter()
        .find(|p| {
            recovery_store::inspect(&root, &p.checkpoint, &|| false)
                .unwrap()
                .pointer("/settings/intensity")
                .and_then(|v| v.as_f64())
                == Some(0.27)
        })
        .expect("The replaced draft must be a separate, inspectable inactive session");
    for value in [0.41, 0.52, 0.63, 0.74] {
        current.settings.intensity = value;
        current.save_recovery_now(&ctx);
        pump(&mut current, &ctx, |a| {
            !a.recovery.busy() && a.recovery.saved_key == recovery_key::key(a).ok()
        });
    }
    assert_eq!(
        recovery_store::inspect(&root, &backup.checkpoint, &|| false)
            .unwrap()
            .pointer("/settings/intensity")
            .and_then(|v| v.as_f64()),
        Some(0.27)
    );
    current.recovery.shutdown(false);
    let history = recovery_store::list_history(&root, &|| false).unwrap();
    assert!(
        history.checkpoints.iter().any(|p| {
            let snapshot = recovery_store::load(&root, &p.checkpoint, &|| false).unwrap();
            snapshot
                .payload
                .pointer("/settings/intensity")
                .and_then(|v| v.as_f64())
                == Some(0.27)
                && snapshot.audio.iter().any(|a| a.samples == current_original)
        }),
        "Replacing a session must retain the previous draft"
    );
}
#[test]
fn dismissed_offer_survives_restart_without_deleting_the_copy() {
    let ctx = egui::Context::default();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Recovery");
    let original = ready(&ctx);
    seed(&root, &original);
    let preferences = temp.path().join("recovery-preferences.json");
    let mut empty = test_fixture::app(&ctx);
    empty.recovery = RecoveryState::load(Some(root.clone()), preferences.clone());
    pump(&mut empty, &ctx, |a| a.recovery.offer);
    empty.recovery.keep_for_later();
    assert!(!empty.recovery.offer);
    empty.recovery.shutdown(true);
    let mut next = test_fixture::app(&ctx);
    next.recovery = RecoveryState::load(Some(root.clone()), preferences);
    pump(&mut next, &ctx, |a| {
        a.recovery.startup_done && !a.recovery.busy()
    });
    assert!(!next.recovery.offer);
    assert!(!next.recovery.listing.checkpoints.is_empty());
    assert!(
        !recovery_store::list_candidates(&root, &|| false)
            .unwrap()
            .checkpoints
            .is_empty()
    );
}
#[test]
fn an_ordinary_capture_never_creates_audio_recovery_data() {
    let ctx = egui::Context::default();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Should not exist");
    let mut app = ready(&ctx);
    app.recovery = RecoveryState::load(Some(root.clone()), temp.path().join("prefs.json"));
    app.capture_session = true;
    for _ in 0..5 {
        app.tick_recovery(&ctx);
    }
    assert!(!root.exists());
    assert!(!app.recovery.busy());
}
#[test]
fn close_requested_waits_for_a_committed_copy_and_normal_exit_is_clean() {
    let ctx = egui::Context::default();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Recovery");
    let mut app = ready(&ctx);
    app.recovery = RecoveryState::load(Some(root.clone()), temp.path().join("prefs.json"));
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1100., 720.),
        )),
        ..Default::default()
    };
    raw.viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let mut output = ctx.run_ui(raw, |ui| app.render_frame(ui));
    output.textures_delta.clear();
    assert!(app.recovery.closing && !app.recovery.allow_close);
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
    );
    pump(&mut app, &ctx, |a| a.recovery.allow_close);
    assert_eq!(app.recovery.saved_key, recovery_key::key(&app).ok());
    app.finish_recovery();
    assert!(
        recovery_store::list_candidates(&root, &|| false)
            .unwrap()
            .checkpoints
            .is_empty()
    );
    assert!(
        !recovery_store::list_history(&root, &|| false)
            .unwrap()
            .checkpoints
            .is_empty()
    );
}

#[test]
fn failed_clear_retains_its_run_identity_until_a_successful_retry() {
    let ctx = egui::Context::default();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Recovery");
    let mut app = ready(&ctx);
    app.recovery = RecoveryState::load(Some(root.clone()), temp.path().join("prefs.json"));
    pump(&mut app, &ctx, |a| {
        a.recovery.startup_done && !a.recovery.busy()
    });
    app.save_recovery_now(&ctx);
    pump(&mut app, &ctx, |a| {
        a.recovery.last_saved.is_some() && !a.recovery.busy()
    });
    let id = app
        .recovery
        .last_saved
        .as_ref()
        .unwrap()
        .checkpoint
        .run_id()
        .to_owned();
    let run = root.join("runs").join(&id);
    let canary = run.join("unknown-user-material.txt");
    std::fs::write(&canary, b"preserve unknown data").unwrap();
    app.recovery.request_clear_current();
    pump(&mut app, &ctx, |a| {
        !a.recovery.busy() && a.recovery.error.is_some()
    });
    assert!(run.is_dir() && canary.is_file());
    assert!(app.recovery.last_saved.is_none());
    assert!(!app.recovery.preferences.enabled);
    // The harness removes only its own canary. Production preserves it.
    std::fs::remove_file(canary).unwrap();
    app.recovery.request_clear_current();
    pump(&mut app, &ctx, |a| {
        !a.recovery.clear_current_requested && !a.recovery.busy()
    });
    assert!(app.recovery.error.is_none(), "{:?}", app.recovery.error);
    assert!(
        !run.exists(),
        "Retry must remember the run after Store and last_saved were cleared"
    );
    assert!(
        recovery_store::list_history(&root, &|| false)
            .unwrap()
            .checkpoints
            .is_empty()
    );
    assert!(app.recovery.message.contains("were cleared"));
}

#[test]
fn clearing_this_windows_recovery_keeps_the_open_audio_and_remembers_autosave_off() {
    let ctx = egui::Context::default();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Recovery");
    let prefs = temp.path().join("prefs.json");
    let mut app = ready(&ctx);
    app.recovery = RecoveryState::load(Some(root.clone()), prefs.clone());
    pump(&mut app, &ctx, |a| {
        a.recovery.startup_done && !a.recovery.busy()
    });
    let original = app.current().unwrap().1.before.samples.clone();
    app.save_recovery_now(&ctx);
    pump(&mut app, &ctx, |a| {
        a.recovery.last_saved.is_some() && !a.recovery.busy()
    });
    app.recovery.request_clear_current();
    pump(&mut app, &ctx, |a| {
        !a.recovery.clear_current_requested && !a.recovery.busy()
    });
    assert!(!app.recovery.preferences.enabled);
    assert!(app.recovery.last_saved.is_none());
    assert_eq!(app.current().unwrap().1.before.samples, original);
    assert!(
        recovery_store::list_history(&root, &|| false)
            .unwrap()
            .checkpoints
            .is_empty()
    );
    let restored = RecoveryState::load(Some(root), prefs);
    assert!(!restored.preferences.enabled);
}
