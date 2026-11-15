//! Real scan/repair jobs through Studio acceptance, without a physical output device.
use super::{
    doctor_state::{Findings, Input},
    interaction_tests::fixture,
    state::{Page, Studio, Target},
};
use crate::{
    audio::Audio,
    gui::plots::Waveform,
    studio::{
        doctor::{self, IssueKind},
        stem_mix::StemKind,
    },
};
use eframe::egui;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) fn damaged_studio() -> Studio {
    let mut studio = fixture(2);
    let mut samples = vec![0.02; 9600];
    for (offset, value) in [0.75, 0.9, 1., 1., 1., 0.9, 0.75].into_iter().enumerate() {
        samples[(1000 + offset) * 2] = value;
    }
    let audio = Arc::new(Audio::new(samples, 48000, 2).unwrap());
    studio.source.as_mut().unwrap().audio = audio.clone();
    let bank = Arc::new(
        studio
            .bank
            .as_ref()
            .unwrap()
            .replace(StemKind::Part1, audio.clone())
            .unwrap(),
    );
    let mut revision = studio.current.clone().unwrap();
    revision.before = audio.clone();
    revision.audio = audio;
    revision.bank = Some(bank);
    studio.accept(revision, false);
    studio.target = Target::Stem(StemKind::Part1);
    studio.page = Page::Repair;
    studio.doctor.visible = true;
    studio.error = None;
    studio
}

pub(super) fn finish(studio: &mut Studio, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while studio.busy() && Instant::now() < deadline {
        studio.poll(ctx);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!studio.busy(), "Doctor job timed out");
}

pub(super) fn scanned(studio: &mut Studio, ctx: &egui::Context) -> usize {
    studio.scan_doctor(ctx);
    finish(studio, ctx);
    assert!(studio.error.is_none(), "{:?}", studio.error);
    studio
        .doctor
        .findings
        .as_ref()
        .unwrap()
        .report
        .issues
        .iter()
        .position(|issue| issue.kind == IssueKind::Clipping && issue.repair.is_some())
        .expect("Fixture must offer a measured declip candidate")
}

#[test]
fn scan_is_read_only_for_source_preview_and_stem_even_with_pending_faders() {
    let ctx = egui::Context::default();
    for target in [
        Target::Source,
        Target::Preview,
        Target::Stem(StemKind::Part1),
    ] {
        let mut studio = damaged_studio();
        studio.target = target;
        let source = studio.source.as_ref().unwrap().audio.clone();
        let current = studio.current.as_ref().unwrap().audio.clone();
        let bank = studio.bank.clone().unwrap();
        studio.mix.tracks[0].gain_db = -4.125;
        studio.mix_dirty = true;
        studio.player.seek(0.04);
        scanned(&mut studio, &ctx);
        assert!(Arc::ptr_eq(&source, &studio.source.as_ref().unwrap().audio));
        assert!(Arc::ptr_eq(
            &current,
            &studio.current.as_ref().unwrap().audio
        ));
        assert!(Arc::ptr_eq(&bank, studio.bank.as_ref().unwrap()));
        assert!(studio.undo.is_empty() && studio.redo.is_empty());
        assert!(studio.mix_dirty);
        assert_eq!(studio.mix.tracks[0].gain_db, -4.125);
        assert_eq!(studio.player.seconds(), 0.04);
    }
}

#[test]
fn repair_prepares_exact_region_then_explicit_apply_and_history_keep_immutable_banks() {
    let ctx = egui::Context::default();
    let mut studio = damaged_studio();
    let index = scanned(&mut studio, &ctx);
    studio.select_doctor_issue(index);
    let source = studio.source.as_ref().unwrap().audio.clone();
    let bank = studio.bank.clone().unwrap();
    let input = bank.track(StemKind::Part1).unwrap().audio.clone();
    let (start, end) = studio.editor.selection.unwrap();
    assert_eq!(studio.player.seconds(), start);
    assert!(!studio.editor.looping);
    assert_eq!(studio.doctor.strength, 0.5);
    studio.preview_doctor_repair(&ctx);
    finish(&mut studio, &ctx);
    assert!(studio.error.is_none(), "{:?}", studio.error);
    let preview = studio.current.as_ref().unwrap();
    assert_eq!(preview.label, "Stem Doctor repair");
    assert!(Arc::ptr_eq(&input, &preview.before));
    assert!(Arc::ptr_eq(&bank, studio.bank.as_ref().unwrap()));
    assert!(Arc::ptr_eq(&source, &studio.source.as_ref().unwrap().audio));
    let start_sample = (start * 48000.).round() as usize * 2;
    let end_sample = (end * 48000.).round() as usize * 2;
    assert_eq!(preview.audio.samples.len(), input.samples.len());
    for (index, (a, b)) in input.samples.iter().zip(&preview.audio.samples).enumerate() {
        if index < start_sample || index >= end_sample || index % 2 == 1 {
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "Unselected sample {index} changed"
            );
        }
    }
    assert_ne!(preview.audio.samples, input.samples);
    let repaired = preview.audio.clone();
    assert!(studio.doctor.findings.is_none());
    assert!(studio.can_apply_preview_to_stem());
    studio.apply_preview_to_stem(&ctx);
    finish(&mut studio, &ctx);
    let edited = studio.bank.clone().unwrap();
    assert!(Arc::ptr_eq(
        &repaired,
        &edited.track(StemKind::Part1).unwrap().audio
    ));
    assert!(Arc::ptr_eq(
        &bank.track(StemKind::Part2).unwrap().audio,
        &edited.track(StemKind::Part2).unwrap().audio
    ));
    studio.history(false);
    assert!(Arc::ptr_eq(&bank, studio.bank.as_ref().unwrap()));
    studio.history(false);
    assert!(Arc::ptr_eq(&input, &studio.current.as_ref().unwrap().audio));
    studio.history(true);
    studio.history(true);
    assert!(Arc::ptr_eq(&edited, studio.bank.as_ref().unwrap()));
    assert!(studio.doctor.findings.is_none());
}

#[test]
fn target_changes_and_revision_history_discard_pending_scan_and_repair_results() {
    let ctx = egui::Context::default();
    for repair in [false, true] {
        for change in [0, 1, 2] {
            let mut studio = damaged_studio();
            let original = studio.current.as_ref().unwrap().audio.clone();
            if repair {
                let index = scanned(&mut studio, &ctx);
                studio.select_doctor_issue(index);
                studio.preview_doctor_repair(&ctx);
            } else {
                studio.scan_doctor(&ctx);
            }
            let expected = match change {
                0 => {
                    studio.target = Target::Stem(StemKind::Part2);
                    original.clone()
                }
                1 => {
                    let mut replacement = studio.current.clone().unwrap();
                    replacement.audio = Arc::new(Audio::new(vec![0.03; 9600], 48000, 2).unwrap());
                    let expected = replacement.audio.clone();
                    studio.accept(replacement, true);
                    expected
                }
                _ => {
                    // Returning to the same Arc via Undo must not revive an old job.
                    studio.accept(studio.current.clone().unwrap(), true);
                    studio.history(false);
                    original.clone()
                }
            };
            finish(&mut studio, &ctx);
            assert!(studio.doctor.findings.is_none());
            assert!(Arc::ptr_eq(
                &expected,
                &studio.current.as_ref().unwrap().audio
            ));
            assert!(studio.status.contains("outdated"), "{}", studio.status);
        }
    }
}

#[test]
fn completed_findings_clear_on_same_target_input_replacement_and_round_trip_target_change() {
    let ctx = egui::Context::default();
    let mut studio = damaged_studio();
    scanned(&mut studio, &ctx);
    studio.target = Target::Source;
    studio.sync_doctor_input();
    studio.target = Target::Stem(StemKind::Part1);
    studio.sync_doctor_input();
    assert!(studio.doctor.findings.is_none());
    scanned(&mut studio, &ctx);
    let replacement = Arc::new(Audio::new(vec![0.04; 9600], 48000, 2).unwrap());
    studio.bank = Some(Arc::new(
        studio
            .bank
            .as_ref()
            .unwrap()
            .replace(StemKind::Part1, replacement)
            .unwrap(),
    ));
    studio.preview_doctor_repair(&ctx);
    assert!(!studio.busy());
    assert!(studio.doctor.findings.is_none());
}

#[test]
fn pending_faders_and_mid_job_fader_changes_preserve_preview_and_controls() {
    let ctx = egui::Context::default();
    for during in [false, true] {
        let mut studio = damaged_studio();
        let index = scanned(&mut studio, &ctx);
        studio.select_doctor_issue(index);
        let original = studio.current.as_ref().unwrap().audio.clone();
        if during {
            studio.preview_doctor_repair(&ctx);
        }
        studio.mix_dirty = true;
        studio.mix.tracks[0].gain_db = -6.125;
        if during {
            finish(&mut studio, &ctx);
            assert!(studio.status.contains("Mix changed"));
        } else {
            studio.preview_doctor_repair(&ctx);
            assert!(!studio.busy());
            assert!(
                studio
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("Update mix preview")
            );
        }
        assert!(Arc::ptr_eq(
            &original,
            &studio.current.as_ref().unwrap().audio
        ));
        assert!(studio.mix_dirty);
        assert_eq!(studio.mix.tracks[0].gain_db, -6.125);
    }
}

#[test]
fn cancellation_rejects_even_already_completed_results_and_allows_rescan() {
    let ctx = egui::Context::default();
    let mut studio = damaged_studio();
    let index = scanned(&mut studio, &ctx);
    studio.select_doctor_issue(index);
    let input = Input::capture(&studio).unwrap();
    let revision = studio.current.clone().unwrap();
    studio.cancel();
    studio.accept_doctor_result(super::doctor_jobs::Result::Preview(input, revision));
    assert!(studio.undo.is_empty());
    studio.scan_doctor(&ctx);
    studio.cancel();
    finish(&mut studio, &ctx);
    assert!(studio.doctor.findings.is_none());
    assert!(studio.undo.is_empty());
    studio.error = None;
    scanned(&mut studio, &ctx);
}

#[test]
fn inspection_only_findings_and_zero_strength_cannot_commit_an_edit() {
    let ctx = egui::Context::default();
    let mut studio = damaged_studio();
    let index = scanned(&mut studio, &ctx);
    studio.select_doctor_issue(index);
    studio.doctor.strength = 0.;
    studio.preview_doctor_repair(&ctx);
    finish(&mut studio, &ctx);
    assert!(studio.undo.is_empty());
    assert!(studio.status.contains("No repair changes"));
    studio.doctor.findings.as_mut().unwrap().report.issues[index].repair = None;
    studio.preview_doctor_repair(&ctx);
    assert!(!studio.busy());
    assert!(studio.error.as_deref().unwrap().contains("inspection only"));
}

pub(super) fn attach_report(studio: &mut Studio) {
    studio.sync_doctor_input();
    let input = Input::capture(studio).unwrap();
    let report = doctor::scan(input.audio(), &|| false).unwrap();
    let waveform = Waveform::new(input.audio(), None);
    studio.doctor.findings = Some(Findings {
        input,
        report,
        waveform,
    });
}

#[test]
fn opening_a_saved_repair_project_clears_transient_doctor_view_and_findings() {
    let mut studio = damaged_studio();
    attach_report(&mut studio);
    let snapshot = studio.snapshot().unwrap();
    assert_eq!(snapshot.view, "repair");
    studio.adopt_project(snapshot).unwrap();
    assert_eq!(studio.page, Page::Repair);
    assert!(!studio.doctor.visible);
    assert!(studio.doctor.findings.is_none());
}
