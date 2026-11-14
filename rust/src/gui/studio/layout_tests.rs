//! R2–R5 regressions use rendered widget bounds and the native accessibility tree.
use super::{
    interaction_tests::{fixture, frame, text},
    state::{Page, Target},
};
use crate::gui::theme;
use eframe::egui;
use std::collections::HashMap;

fn visible<'a>(output: &'a egui::FullOutput, label: &str) -> Vec<&'a egui::epaint::ClippedShape> {
    output
        .shapes
        .iter()
        .filter(|shape| match &shape.shape {
            egui::Shape::Text(text) => {
                text.galley.text() == label
                    && shape.clip_rect.contains_rect(text.visual_bounding_rect())
            }
            _ => false,
        })
        .collect()
}

#[test]
fn active_tool_editors_and_single_preview_export_actions_are_visible_at_both_sizes() {
    for size in [egui::vec2(1100., 720.), egui::vec2(1480., 940.)] {
        for advanced in [false, true] {
            for (page, action, editors) in [
                (
                    Page::Voice,
                    "Preview voice cleanup",
                    ["Noise reduction", "Soften S sounds (de-ess)"],
                ),
                (Page::Repair, "Preview spectral repair", ["Start", "End"]),
            ] {
                let ctx = egui::Context::default();
                theme::apply(&ctx);
                let mut studio = fixture(4);
                studio.page = page;
                let mut output = None;
                for _ in 0..5 {
                    output = Some(frame(&ctx, &mut studio, advanced, size, 332., vec![]).0);
                }
                let output = output.as_ref().unwrap();
                for label in [
                    action,
                    "Export current preview",
                    "Choose audio…",
                    "Process this source",
                ] {
                    assert_eq!(
                        visible(output, label).len(),
                        1,
                        "Missing, duplicated or clipped {label} at {size:?}, advanced={advanced}"
                    );
                }
                let preview = text(output, action).unwrap().shape.visual_bounding_rect();
                let export = text(output, "Export current preview")
                    .unwrap()
                    .shape
                    .visual_bounding_rect();
                assert!(
                    preview.bottom() < export.top(),
                    "Preview must come before Export in the same pinned dock"
                );
                assert!(preview.left() > size.x - 360. && export.left() > size.x - 360.);
                for label in editors {
                    let editor = visible(output, label);
                    assert_eq!(editor.len(), 1, "Editor hidden/duplicated: {label}");
                    assert!(
                        editor[0].shape.visual_bounding_rect().right() < size.x - 332.,
                        "Editor is still in the narrow sidebar"
                    );
                }
            }
        }
    }
}

#[test]
fn compact_four_part_controls_fit_without_sacrificing_names_or_adding_audio_changes() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = fixture(4);
    studio.mix.tracks[0].gain_db = -3.12506789;
    let before = serde_json::to_value(&studio.mix).unwrap();
    let size = egui::vec2(1100., 720.);
    let mut output = None;
    for _ in 0..8 {
        output = Some(frame(&ctx, &mut studio, false, size, 332., vec![]).0);
    }
    let output = output.as_ref().unwrap();
    for index in 1..=4 {
        let name = format!("Part {index:03}");
        let row = text(output, &name).unwrap_or_else(|| panic!("Part {index} is below the fold"));
        assert!(
            row.clip_rect
                .contains_rect(row.shape.visual_bounding_rect()),
            "Part {index} clipped"
        );
    }
    assert_eq!(
        visible(output, "Mute").len(),
        4,
        "All four mute controls should be immediately visible"
    );
    assert_eq!(
        visible(output, "Solo").len(),
        4,
        "All four solo controls should be immediately visible"
    );
    assert_eq!(serde_json::to_value(&studio.mix).unwrap(), before);
    assert_eq!(studio.volume, 0.);
    assert!(!studio.mix_dirty && !studio.busy());
}

#[test]
fn studio_export_shows_the_shared_human_labels_and_preserves_format_tokens() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = fixture(2);
    for format in crate::config::FORMATS {
        studio.export_format = (*format).into();
        let mut settled = None;
        for _ in 0..5 {
            settled = Some(
                frame(
                    &ctx,
                    &mut studio,
                    false,
                    egui::vec2(1100., 720.),
                    332.,
                    vec![],
                )
                .0,
            );
        }
        let output = settled.unwrap();
        let label = crate::gui::mastering_controls::format_label(format);
        assert_eq!(
            visible(&output, label).len(),
            1,
            "Format label missing: {label}"
        );
        assert!(
            visible(&output, format).is_empty(),
            "Internal format token leaked into the UI"
        );
        assert_eq!(studio.export_format, *format);
    }
}

fn accessible_label(
    id: egui::accesskit::NodeId,
    nodes: &HashMap<egui::accesskit::NodeId, &egui::accesskit::Node>,
    depth: usize,
) -> String {
    if depth > 5 {
        return String::new();
    }
    let Some(node) = nodes.get(&id) else {
        return String::new();
    };
    if let Some(label) = node.label() {
        return label.to_owned();
    }
    node.labelled_by()
        .iter()
        .map(|id| accessible_label(*id, nodes, depth + 1))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn native_stem_sliders_and_numeric_editors_name_the_actual_part_gain_and_units() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    ctx.enable_accesskit();
    let mut studio = fixture(4);
    let names: Vec<_> = studio
        .bank
        .as_ref()
        .unwrap()
        .tracks
        .iter()
        .map(|stem| {
            studio
                .bank
                .as_ref()
                .unwrap()
                .display_name(stem.kind)
                .into_owned()
        })
        .collect();
    let output = frame(
        &ctx,
        &mut studio,
        false,
        egui::vec2(1100., 720.),
        332.,
        vec![],
    )
    .0;
    let tree = output
        .platform_output
        .accesskit_update
        .expect("Native accessibility tree required");
    let nodes: HashMap<_, _> = tree.nodes.iter().map(|(id, node)| (*id, node)).collect();
    for name in names {
        for role in [
            egui::accesskit::Role::Slider,
            egui::accesskit::Role::SpinButton,
        ] {
            assert!(
                tree.nodes.iter().any(|(id, node)| node.role() == role && {
                    let label = accessible_label(*id, &nodes, 0);
                    label.contains(&name) && label.contains("gain, dB")
                }),
                "Missing accessible {role:?} name for {name}"
            );
        }
    }
}

#[test]
fn revealing_tool_controls_retains_precise_settings_targets_and_muted_audition() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = fixture(4);
    studio.target = Target::Preview;
    studio.voice.noise_reduction = 0.312506789;
    studio.voice.deess = 0.287512349;
    studio.repair.start_seconds = 0.012506789;
    studio.repair.end_seconds = 0.091234567;
    studio.repair.low_hz = 1234.56789;
    studio.repair.reduction_db = -7.12506789;
    studio.mix.tracks[0].gain_db = -3.12506789;
    let voice = serde_json::to_value(&studio.voice).unwrap();
    let repair = serde_json::to_value(&studio.repair).unwrap();
    let mix = serde_json::to_value(&studio.mix).unwrap();
    for advanced in [false, true, false] {
        for page in [Page::Voice, Page::Repair, Page::Stems] {
            studio.page = page;
            frame(
                &ctx,
                &mut studio,
                advanced,
                egui::vec2(1100., 720.),
                332.,
                vec![],
            );
            assert_eq!(serde_json::to_value(&studio.voice).unwrap(), voice);
            assert_eq!(serde_json::to_value(&studio.repair).unwrap(), repair);
            assert_eq!(serde_json::to_value(&studio.mix).unwrap(), mix);
            assert!(matches!(studio.target, Target::Preview));
            assert_eq!(studio.volume, 0.);
        }
    }
}

fn activate_compact(
    ctx: &egui::Context,
    studio: &mut super::Studio,
    output: &egui::FullOutput,
    label: &str,
) -> egui::FullOutput {
    let pos = text(output, label)
        .unwrap()
        .shape
        .visual_bounding_rect()
        .center();
    let event = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        ctx,
        studio,
        false,
        egui::vec2(1100., 720.),
        332.,
        vec![egui::Event::PointerMoved(pos), event(true)],
    );
    let mut output = frame(
        ctx,
        studio,
        false,
        egui::vec2(1100., 720.),
        332.,
        vec![event(false)],
    )
    .0;
    for _ in 0..10 {
        output = frame(ctx, studio, false, egui::vec2(1100., 720.), 332., vec![]).0;
    }
    output
}

#[test]
fn compact_comparison_and_part_plots_are_explicit_and_do_not_hide_playback_or_change_audio() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    ctx.all_styles_mut(|style| style.animation_time = 0.);
    let mut studio = fixture(4);
    let before = serde_json::to_value(&studio.mix).unwrap();
    let mut output = frame(
        &ctx,
        &mut studio,
        false,
        egui::vec2(1100., 720.),
        332.,
        vec![],
    )
    .0;
    for _ in 0..5 {
        output = frame(
            &ctx,
            &mut studio,
            false,
            egui::vec2(1100., 720.),
            332.,
            vec![],
        )
        .0;
    }
    assert!(text(&output, "BEFORE").is_none());
    output = activate_compact(&ctx, &mut studio, &output, "Comparison waveform");
    for label in [
        "BEFORE",
        "PREVIEW",
        "Play",
        "Stop",
        "A · Before",
        "B · Preview",
        "Volume",
        "Level match",
    ] {
        assert_eq!(
            visible(&output, label).len(),
            1,
            "Expanded compact dock hid {label}"
        );
    }
    let before_lane = text(&output, "BEFORE")
        .unwrap()
        .shape
        .visual_bounding_rect();
    let after_lane = text(&output, "PREVIEW")
        .unwrap()
        .shape
        .visual_bounding_rect();
    assert!(
        after_lane.top() - before_lane.bottom() >= 12.,
        "Comparison traces need useful space between labels"
    );
    output = activate_compact(&ctx, &mut studio, &output, "Comparison waveform");
    output = activate_compact(&ctx, &mut studio, &output, "Show part waveforms");
    assert!(
        !visible(&output, "PART").is_empty(),
        "Part plots did not become visible"
    );
    output = activate_compact(&ctx, &mut studio, &output, "Show part waveforms");
    assert_eq!(visible(&output, "Mute").len(), 4);
    assert_eq!(visible(&output, "Solo").len(), 4);
    assert_eq!(serde_json::to_value(&studio.mix).unwrap(), before);
    assert!(!studio.mix_dirty && !studio.busy());
    assert_eq!(studio.volume, 0.);
}
