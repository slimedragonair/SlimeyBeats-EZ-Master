//! Pointer-driven native widgets, clipping bounds and narrow workspace states.
use super::{
    controls,
    doctor_tests::{attach_report, damaged_studio, finish},
    state::{Page, Studio},
    workspace,
};
use crate::gui::theme;
use eframe::egui;
use std::sync::{Arc, atomic::Ordering};

fn frame(
    ctx: &egui::Context,
    studio: &mut Studio,
    width: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 920.),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            egui::Panel::right("doctor-test-settings")
                .exact_size(290.)
                .show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(10)
                        .show(ui, |ui| controls::show(studio, ui, None, ctx, false));
                });
            egui::CentralPanel::default().show(ui, |ui| {
                workspace::show(studio, ui, ctx, false);
            });
        },
    );
    output.textures_delta.clear();
    output
}

fn settle(ctx: &egui::Context, studio: &mut Studio, width: f32) -> egui::FullOutput {
    let mut output = frame(ctx, studio, width, vec![]);
    for _ in 0..5 {
        output = frame(ctx, studio, width, vec![]);
    }
    output
}

fn visible_rect(output: &egui::FullOutput, label: &str) -> egui::Rect {
    let candidates: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|item| match &item.shape {
            egui::Shape::Text(text) if text.galley.text().contains(label) => {
                let rect = item.shape.visual_bounding_rect();
                Some((text.galley.text() == label, rect, item.clip_rect))
            }
            _ => None,
        })
        .collect();
    candidates
        .iter()
        .filter(|(_, rect, clip)| clip.contains_rect(*rect))
        .max_by_key(|(exact, _, _)| *exact)
        .map(|(_, rect, _)| *rect)
        .unwrap_or_else(|| panic!("Missing fully visible text: {label}; matches: {candidates:?}"))
}

fn click(
    ctx: &egui::Context,
    studio: &mut Studio,
    width: f32,
    output: &egui::FullOutput,
    label: &str,
) -> egui::FullOutput {
    let pos = visible_rect(output, label).center();
    let event = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        ctx,
        studio,
        width,
        vec![egui::Event::PointerMoved(pos), event(true)],
    );
    frame(ctx, studio, width, vec![event(false)])
}

#[test]
fn narrow_doctor_page_keeps_actions_transport_strength_and_measured_finding_inside_clips() {
    for width in [640., 1000.] {
        for scale in [1., 1.25] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(scale);
            theme::apply(&ctx);
            let mut studio = damaged_studio();
            attach_report(&mut studio);
            let input = studio.current.as_ref().unwrap().audio.clone();
            let output = settle(&ctx, &mut studio, width);
            for label in [
                "Stem Doctor",
                "Scan selected audio",
                "Preview selected repair",
                "Repair strength",
                "Low-mid buildup is a tonal heuristic",
                "Full-scale sample candidate",
                "SCANNED INPUT",
                "ISSUE RANGES",
                "Play",
                "Stop",
                "A · Before",
                "B · Preview",
                "Export current preview",
            ] {
                let rect = visible_rect(&output, label);
                assert!(
                    rect.left() >= 0. && rect.right() <= width + 0.5,
                    "{width} at {scale}: {label} escaped {rect:?}"
                );
            }
            assert!(Arc::ptr_eq(&input, &studio.current.as_ref().unwrap().audio));
            assert_eq!(studio.doctor.strength, 0.5);
            assert!(!studio.busy());
        }
    }
}

#[test]
fn pointer_scan_select_repair_then_apply_use_existing_preview_and_stem_flow() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = damaged_studio();
    studio.page = Page::Stems;
    studio.doctor.visible = false;
    let output = settle(&ctx, &mut studio, 1000.);
    click(&ctx, &mut studio, 1000., &output, "Stem Doctor");
    let output = settle(&ctx, &mut studio, 1000.);
    click(&ctx, &mut studio, 1000., &output, "Scan selected audio");
    assert!(studio.busy());
    finish(&mut studio, &ctx);
    assert!(studio.doctor.findings.is_some());
    let bank = studio.bank.clone().unwrap();
    let output = settle(&ctx, &mut studio, 1000.);
    click(
        &ctx,
        &mut studio,
        1000.,
        &output,
        "Full-scale sample candidate",
    );
    assert!(studio.doctor.selected.is_some());
    let start = studio.editor.selection.unwrap().0;
    assert_eq!(studio.player.seconds(), start);
    let output = settle(&ctx, &mut studio, 1000.);
    click(&ctx, &mut studio, 1000., &output, "Preview selected repair");
    finish(&mut studio, &ctx);
    assert!(studio.error.is_none(), "{:?}", studio.error);
    assert!(Arc::ptr_eq(&bank, studio.bank.as_ref().unwrap()));
    assert_eq!(studio.current.as_ref().unwrap().label, "Stem Doctor repair");
    let output = settle(&ctx, &mut studio, 1000.);
    click(
        &ctx,
        &mut studio,
        1000.,
        &output,
        "Apply preview to selected stem",
    );
    finish(&mut studio, &ctx);
    assert_eq!(
        studio.current.as_ref().unwrap().label,
        "Mix with edited stem"
    );
    assert!(!Arc::ptr_eq(&bank, studio.bank.as_ref().unwrap()));
}

#[test]
fn actual_target_chooser_invalidates_findings_and_hides_old_markers() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = damaged_studio();
    attach_report(&mut studio);
    let output = settle(&ctx, &mut studio, 1000.);
    let output = click(&ctx, &mut studio, 1000., &output, "Part 001");
    let output = if output.shapes.iter().any(|item| matches!(&item.shape, egui::Shape::Text(text) if text.galley.text() == "Original source")) { output } else { frame(&ctx, &mut studio, 1000., vec![]) };
    click(&ctx, &mut studio, 1000., &output, "Original source");
    let output = settle(&ctx, &mut studio, 1000.);
    assert!(matches!(studio.target, super::state::Target::Source));
    assert!(studio.doctor.findings.is_none());
    assert!(!output.shapes.iter().any(|item| matches!(&item.shape, egui::Shape::Text(text) if text.galley.text().contains("Full-scale sample candidate"))));
    assert!(!studio.busy());
}

#[test]
fn busy_and_pending_mix_controls_reject_pointer_repair_and_cancel_remains_reachable() {
    for width in [640., 1000.] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut studio = damaged_studio();
        attach_report(&mut studio);
        studio.select_doctor_issue(0);
        let original = studio.current.as_ref().unwrap().audio.clone();
        studio.mix_dirty = true;
        studio.mix.tracks[0].gain_db = -6.;
        let output = settle(&ctx, &mut studio, width);
        click(&ctx, &mut studio, width, &output, "Preview selected repair");
        assert!(!studio.busy());
        studio.jobs.active = Some(999);
        studio.jobs.cancel.store(false, Ordering::Relaxed);
        let output = settle(&ctx, &mut studio, width);
        click(&ctx, &mut studio, width, &output, "Scan selected audio");
        assert_eq!(studio.jobs.active, Some(999));
        let output = settle(&ctx, &mut studio, width);
        click(&ctx, &mut studio, width, &output, "Cancel doctor job");
        assert!(studio.jobs.cancel.load(Ordering::Relaxed));
        assert!(Arc::ptr_eq(
            &original,
            &studio.current.as_ref().unwrap().audio
        ));
        assert!(studio.mix_dirty);
        assert_eq!(studio.mix.tracks[0].gain_db, -6.);
    }
}

#[test]
fn spectral_repair_and_doctor_tabs_keep_independent_views_without_starting_jobs() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = damaged_studio();
    attach_report(&mut studio);
    let output = settle(&ctx, &mut studio, 1000.);
    click(&ctx, &mut studio, 1000., &output, "Spectral repair");
    assert!(!studio.doctor.visible);
    let output = settle(&ctx, &mut studio, 1000.);
    visible_rect(&output, "Preview spectral repair");
    click(&ctx, &mut studio, 1000., &output, "Stem Doctor");
    let output = settle(&ctx, &mut studio, 1000.);
    visible_rect(&output, "Full-scale sample candidate");
    assert!(studio.doctor.visible && studio.doctor.findings.is_some());
    assert!(!studio.busy() && studio.undo.is_empty());
}

#[test]
fn sample_scale_finding_labels_show_distinct_endpoints_and_three_frames_in_narrow_panels() {
    for width in [640., 1000.] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut studio = damaged_studio();
        attach_report(&mut studio);
        let output = settle(&ctx, &mut studio, width);
        for text in ["0:00.020875–0:00.020938", "3 frames per channel", " ms)"] {
            visible_rect(&output, text);
        }
        assert!(!studio.busy() && studio.undo.is_empty());
    }
}

#[test]
fn hovering_a_sample_scale_waveform_marker_shows_precise_endpoints_and_frame_count() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    ctx.all_styles_mut(|style| {
        style.interaction.tooltip_delay = 0.;
        style.interaction.show_tooltips_only_when_still = false;
    });
    let mut studio = damaged_studio();
    attach_report(&mut studio);
    let mut time = 0.;
    let mut render = |events| {
        time += 0.1;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 300.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| super::doctor_waveform::show(&mut studio, ui),
        );
        output.textures_delta.clear();
        output
    };
    let mut output = render(vec![]);
    let marker_color =
        super::doctor_controls::issue_color(crate::studio::doctor::IssueKind::Clipping)
            .gamma_multiply(0.7);
    let marker = output
        .shapes
        .iter()
        .find_map(|item| match &item.shape {
            egui::Shape::Rect(rect) if rect.fill == marker_color => Some(rect.rect.center()),
            _ => None,
        })
        .expect("Measured clipping marker must be painted");
    for _ in 0..5 {
        output = render(vec![egui::Event::PointerMoved(marker)]);
    }
    for text in ["0:00.020875–0:00.020938", "3 frames per channel", " ms)"] {
        visible_rect(&output, text);
    }
    assert!(!studio.busy() && studio.undo.is_empty());
}
