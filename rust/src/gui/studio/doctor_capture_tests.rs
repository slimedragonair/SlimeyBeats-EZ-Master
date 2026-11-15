use super::*;
use crate::gui::studio::doctor_tests::{damaged_studio, finish};
use std::sync::Arc;

#[test]
fn capture_waits_for_load_scans_in_background_once_and_never_prepares_a_repair() {
    let ctx = egui::Context::default();
    let mut studio = damaged_studio();
    let source = studio.source.take();
    let preview = studio.current.as_ref().unwrap().audio.clone();
    request(&ctx);
    studio.poll(&ctx);
    assert!(!studio.busy());
    assert!(!status(&ctx).unwrap().doctor_scan_ready);
    studio.source = source;
    studio.poll(&ctx);
    assert!(studio.busy());
    assert!(!status(&ctx).unwrap().doctor_scan_ready);
    finish(&mut studio, &ctx);
    let captured = status(&ctx).unwrap();
    assert!(captured.doctor_scan_ready);
    assert!(captured.doctor_issue_count > 0);
    assert!(studio.doctor.visible && studio.page == Page::Repair);
    assert!(studio.target == Target::Source);
    assert!(studio.doctor.selected.is_some());
    for _ in 0..5 {
        studio.poll(&ctx);
        assert!(!studio.busy());
    }
    assert!(Arc::ptr_eq(
        &preview,
        &studio.current.as_ref().unwrap().audio
    ));
    assert!(studio.undo.is_empty() && studio.redo.is_empty());
}

#[test]
fn normal_context_never_queues_diagnostic_work() {
    let ctx = egui::Context::default();
    let mut studio = damaged_studio();
    for _ in 0..5 {
        studio.poll(&ctx);
    }
    assert!(status(&ctx).is_none());
    assert!(!studio.busy() && studio.doctor.findings.is_none());
}

#[test]
fn failed_or_stale_scans_are_terminal_and_cannot_qualify_a_capture() {
    let ctx = egui::Context::default();
    let mut studio = damaged_studio();
    request(&ctx);
    studio.poll(&ctx);
    studio.target = Target::Preview;
    finish(&mut studio, &ctx);
    let result = status(&ctx).unwrap();
    assert!(!result.doctor_scan_ready && result.error.is_some());
    studio.poll(&ctx);
    assert!(!studio.busy());

    let ctx = egui::Context::default();
    request(&ctx);
    studio.error = Some("Synthetic load failure".into());
    studio.poll(&ctx);
    assert!(
        status(&ctx)
            .unwrap()
            .error
            .unwrap()
            .contains("Synthetic load failure")
    );
}
