//! Controlled completion races supplement real disk/process recovery tests.
use super::super::{recovery_capture, recovery_restore, test_fixture};
use super::*;
use recovery_worker::{Completed, Outcome, Worker};

fn mocked() -> (
    RecoveryState,
    mpsc::Sender<Completed>,
    mpsc::Receiver<Command>,
) {
    let (sender, commands) = mpsc::channel();
    let (events, receiver) = mpsc::channel();
    let mut state = RecoveryState::for_tests();
    state.worker = Some(Worker { sender, receiver });
    state.operation = 1;
    state.active = Some(1);
    (state, events, commands)
}
fn reference(generation: u64) -> CheckpointRef {
    serde_json::from_value(serde_json::json!({"run_id":"run-1-2-3","generation":generation}))
        .unwrap()
}
#[test]
fn a_late_restore_completion_after_cancel_is_never_adopted() {
    let ctx = egui::Context::default();
    let app = test_fixture::app(&ctx);
    let capture = recovery_capture::capture(&app).unwrap();
    let prepared = recovery_restore::prepare(capture.payload, capture.audio).unwrap();
    let (mut state, events, _commands) = mocked();
    state.restoring = true;
    state.cancel();
    events
        .send(Completed {
            id: 1,
            result: Ok(Outcome::Restored(Box::new(prepared))),
        })
        .unwrap();
    assert!(state.poll().is_none());
    assert!(!state.restore_completed && !state.restoring);
    state.worker = None;
}
#[test]
fn failed_value_reversions_clear_deadlines_instead_of_repainting_or_retrying_forever() {
    let (mut state, events, _commands) = mocked();
    state.active_save_key = Some(9);
    events
        .send(Completed {
            id: 1,
            result: Err("Injected full disk".into()),
        })
        .unwrap();
    state.poll();
    let now = Instant::now();
    state.observe(9, now);
    assert_eq!(state.failed_key, Some(9));
    assert!(!state.due(now + Duration::from_secs(100)));
    state.observe(10, now);
    assert!(state.due(now + Duration::from_secs(2)));
    state.observe(9, now + Duration::from_secs(3));
    assert!(state.changed_at.is_none() && state.dirty_since.is_none());
    state.worker = None;
}
#[test]
fn stale_inspection_cannot_label_a_new_selection_with_an_old_payload() {
    let (mut state, events, _commands) = mocked();
    state.selected = Some(reference(2));
    events
        .send(Completed {
            id: 1,
            result: Ok(Outcome::Inspected(
                reference(1),
                serde_json::json!({"old":"private"}),
            )),
        })
        .unwrap();
    state.poll();
    assert!(state.inspected.is_none());
    state.worker = None;
}

#[test]
fn closing_waits_for_clear_even_after_autosave_is_disabled_and_never_recreates_a_copy() {
    let ctx = egui::Context::default();
    super::super::theme::apply(&ctx);
    let mut app = test_fixture::app(&ctx);
    let (mut state, events, commands) = mocked();
    state.preferences.enabled = false;
    state.clearing_current = true;
    app.recovery = state;
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
    assert!(app.recovery.closing && app.recovery.closing_for_clear && !app.recovery.allow_close);
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
    );
    events
        .send(Completed {
            id: 1,
            result: Ok(Outcome::CurrentCleared(None)),
        })
        .unwrap();
    app.tick_recovery(&ctx);
    assert!(app.recovery.allow_close);
    assert!(!app.recovery.preferences.enabled);
    assert!(
        commands
            .try_iter()
            .all(|command| !matches!(command, Command::Run(_, Work::Save(..), _))),
        "Closing after Clear must not create new audio copies"
    );
    app.recovery.worker = None;
}

#[test]
fn os_close_cannot_cancel_the_only_backup_while_restore_is_in_progress() {
    let ctx = egui::Context::default();
    super::super::theme::apply(&ctx);
    let mut app = test_fixture::app(&ctx);
    let (mut state, _events, _commands) = mocked();
    state.restoring = true;
    state.preferences.enabled = false;
    app.recovery = state;
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
    assert!(app.recovery.closing_for_restore && !app.recovery.allow_close);
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
    );
    assert!(!app.recovery.cancel.load(Ordering::Relaxed));
    app.recovery.worker = None;
}

#[test]
fn cancelling_initial_history_scan_leaves_refresh_manual() {
    let (mut state, events, _commands) = mocked();
    state.startup_done = false;
    state.cancel();
    events
        .send(Completed {
            id: 1,
            result: Err("Cancelled".into()),
        })
        .unwrap();
    state.poll();
    assert!(state.startup_done);
    assert!(!state.busy());
    state.worker = None;
}
