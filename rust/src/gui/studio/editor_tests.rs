//! Exercise controller commits, pointer selections and the real compact layout.
use super::{
    editor_controls, editor_timeline,
    interaction_tests::{fixture, frame, text},
    state::Studio,
};
use crate::{
    gui::theme,
    studio::{region_edit::RegionOperation, stem_mix::StemKind},
};
use eframe::egui;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn finish(studio: &mut Studio, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while studio.busy() && Instant::now() < deadline {
        studio.poll(ctx);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!studio.busy(), "Editor job timed out");
    assert!(studio.error.is_none(), "{:?}", studio.error);
}

fn headless(mut output: egui::FullOutput) -> egui::FullOutput {
    output.textures_delta.clear();
    output
}

#[test]
fn region_edit_commits_only_selected_part_and_undo_redo_restore_the_exact_banks() {
    let ctx = egui::Context::default();
    let mut studio = fixture(2);
    let mut named = (**studio.bank.as_ref().unwrap()).clone();
    named
        .set_display_name(StemKind::Part1, format!("{}AB", "龍".repeat(170)))
        .unwrap();
    let named = Arc::new(named);
    studio.bank = Some(named.clone());
    studio.current.as_mut().unwrap().bank = Some(named);
    let original = studio.bank.clone().unwrap();
    let source = studio.source.as_ref().unwrap().audio.clone();
    studio.editor.selected = Some(StemKind::Part1);
    studio
        .editor
        .select_time(0.02, 0.06, 48000, source.frames());
    studio.player.seek(0.03);
    studio.edit_selected_region(RegionOperation::Silence, &ctx);
    finish(&mut studio, &ctx);
    let edited = studio.bank.clone().unwrap();
    assert!(studio.current.as_ref().unwrap().label.len() <= 512);
    let samples = &edited.track(StemKind::Part1).unwrap().audio.samples;
    assert!(samples[1920..5760].iter().all(|sample| *sample == 0.));
    assert!(samples[..1920].iter().all(|sample| *sample == 0.02));
    assert!(samples[5760..].iter().all(|sample| *sample == 0.02));
    assert!(Arc::ptr_eq(
        &edited.track(StemKind::Part2).unwrap().audio,
        &original.track(StemKind::Part2).unwrap().audio
    ));
    assert!(Arc::ptr_eq(&source, &studio.source.as_ref().unwrap().audio));
    assert_eq!(studio.player.seconds(), 0.03);
    assert!(!studio.mix_dirty);
    assert!(studio.snapshot().is_ok());
    let folder = tempfile::tempdir().unwrap();
    let project =
        crate::studio_native::session::save(&studio.snapshot().unwrap(), folder.path(), &|| false)
            .unwrap();
    let reopened = crate::studio_native::session::StudioDocument::load(&project).unwrap();
    assert_eq!(
        reopened
            .stems
            .as_ref()
            .unwrap()
            .display_name(StemKind::Part1),
        original.display_name(StemKind::Part1)
    );
    assert_eq!(
        reopened
            .stems
            .as_ref()
            .unwrap()
            .track(StemKind::Part1)
            .unwrap()
            .audio
            .samples,
        *samples
    );
    assert_eq!(reopened.source.samples, source.samples);
    studio.history(false);
    assert!(Arc::ptr_eq(studio.bank.as_ref().unwrap(), &original));
    studio.history(true);
    assert!(Arc::ptr_eq(studio.bank.as_ref().unwrap(), &edited));
    assert!(studio.snapshot().is_ok());
}

#[test]
fn pending_faders_busy_and_empty_selections_never_queue_or_discard_an_edit() {
    let ctx = egui::Context::default();
    let mut studio = fixture(2);
    let bank = studio.bank.clone().unwrap();
    studio.editor.selected = Some(StemKind::Part1);
    studio.edit_selected_region(RegionOperation::Silence, &ctx);
    assert!(!studio.busy());
    assert!(studio.error.take().unwrap().contains("Select a stem"));
    studio.editor.select_time(0.01, 0.05, 48000, 4800);
    studio.mix.tracks[0].gain_db = -4.12506789;
    studio.mix_dirty = true;
    studio.edit_selected_region(RegionOperation::FadeOut, &ctx);
    assert!(!studio.busy());
    assert!(studio.error.take().unwrap().contains("Update mix preview"));
    assert_eq!(studio.mix.tracks[0].gain_db, -4.12506789);
    studio.jobs.active = Some(987);
    studio.edit_selected_region(RegionOperation::Reverse, &ctx);
    assert_eq!(studio.jobs.active, Some(987));
    assert!(Arc::ptr_eq(&bank, studio.bank.as_ref().unwrap()));
}

#[test]
fn timeline_lane_pointer_selection_selects_the_part_and_snaps_a_time_range() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = fixture(2);
    let mut render = |events| {
        headless(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 500.),
                )),
                events,
                ..Default::default()
            },
            |ui| editor_timeline::show(&mut studio, ui, true),
        ))
    };
    for _ in 0..4 {
        render(vec![]);
    }
    let press = egui::pos2(400., 60.);
    let end = egui::pos2(700., 60.);
    let event = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    render(vec![egui::Event::PointerMoved(press), event(press, true)]);
    render(vec![egui::Event::PointerMoved(end)]);
    render(vec![event(end, false)]);
    assert_eq!(studio.editor.selected, Some(StemKind::Part1));
    let (start, finish) = studio
        .editor
        .selection
        .expect("Dragging the waveform must select time");
    assert!(
        start > 0. && finish > start && finish < 0.1,
        "{start}..{finish}"
    );
    assert_eq!((start * 48000.).round(), start * 48000.);
    assert!(!studio.busy() && !studio.mix_dirty);
}

#[test]
fn compact_timeline_keeps_navigation_first_lane_and_export_reachable_without_changing_audio() {
    for size in [egui::vec2(1100., 720.), egui::vec2(1480., 940.)] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut studio = fixture(100);
        let audio = studio.current.as_ref().unwrap().audio.clone();
        let mut output = None;
        for _ in 0..6 {
            output = Some(frame(&ctx, &mut studio, false, size, 332., vec![]).0);
        }
        let output = output.unwrap();
        for label in [
            "Timeline",
            "Mixer",
            "Fit song",
            "ALIGNED STEMS",
            "Mute",
            "Solo",
            "Play",
            "Export current preview",
        ] {
            let shape =
                text(&output, label).unwrap_or_else(|| panic!("Missing {label} at {size:?}"));
            assert!(
                shape
                    .clip_rect
                    .expand(1.)
                    .contains_rect(shape.shape.visual_bounding_rect()),
                "Clipped {label}"
            );
        }
        assert!(Arc::ptr_eq(&audio, &studio.current.as_ref().unwrap().audio));
        assert!(!studio.mix_dirty && !studio.busy());
    }
}

#[test]
fn numeric_selection_can_be_entered_and_unchanged_paint_keeps_precise_values() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = fixture(2);
    studio.editor.inspector_edits = true;
    studio.editor.selected = Some(StemKind::Part2);
    studio.editor.draft_start = 0.012506789;
    studio.editor.draft_end = 0.091234567;
    studio.mix.tracks[1].gain_db = -3.12506789;
    for _ in 0..4 {
        let _ = headless(ctx.run_ui(egui::RawInput::default(), |ui| {
            editor_controls::show(&mut studio, ui, &ctx, false)
        }));
    }
    assert_eq!(studio.editor.draft_start, 0.012506789);
    assert_eq!(studio.editor.draft_end, 0.091234567);
    assert_eq!(studio.mix.tracks[1].gain_db, -3.12506789);
    assert!(!studio.busy());
}

#[test]
fn tiny_selections_keep_sample_edits_but_disable_audition_loops_and_fit_a_readable_view() {
    let mut studio = fixture(2);
    studio.editor.select_time(0.02, 0.08, 48000, 4800);
    studio.editor.looping = true;
    studio.update_editor_loop();
    assert!(studio.player.loop_range().is_some());
    studio
        .editor
        .select_time(0.02, 0.02 + 1. / 48000., 48000, 4800);
    studio.update_editor_loop();
    assert!(studio.editor.selection.is_some());
    assert!(!studio.editor.looping);
    assert!(studio.player.loop_range().is_none());
    studio.editor.fit_selection(0.1);
    let (start, end) = studio.editor.range(0.1);
    assert!(end - start >= 0.009999);
    assert_eq!(
        super::editor_state::time_label_precise(59.999999, 2),
        "1:00.00"
    );
    assert_ne!(
        super::editor_state::time_label_precise(0.0201, 4),
        super::editor_state::time_label_precise(0.0214, 4)
    );
}

#[test]
fn typing_exact_seconds_is_not_replaced_by_frame_snapping_between_keystrokes() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = fixture(2);
    studio.editor.inspector_edits = true;
    studio.editor.draft_start = 0.01;
    studio.editor.draft_end = 0.09;
    let render = |studio: &mut Studio, events| {
        headless(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 600.),
                )),
                events,
                ..Default::default()
            },
            |ui| editor_controls::show(studio, ui, &ctx, false),
        ))
    };
    let mut output = render(&mut studio, vec![]);
    for _ in 0..4 {
        output = render(&mut studio, vec![]);
    }
    let pos = text(&output, "0.01")
        .expect("Start time editor")
        .shape
        .visual_bounding_rect()
        .center();
    for pressed in [true, false] {
        output = render(
            &mut studio,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    assert!(ctx.egui_wants_keyboard_input());
    let key = |key, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    let _ = render(
        &mut studio,
        vec![key(egui::Key::A, egui::Modifiers::COMMAND)],
    );
    for digit in "0.0123437".chars() {
        output = render(&mut studio, vec![egui::Event::Text(digit.to_string())]);
        assert_eq!(
            studio.editor.draft_start, 0.01,
            "Focused seconds committed during typing"
        );
    }
    assert!(
        text(&output, "0.0123437").is_some(),
        "Typed text was replaced or lost"
    );
    let _ = render(
        &mut studio,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    assert_eq!(
        studio.editor.selection.unwrap().0,
        (0.0123437_f64 * 48000.).round() / 48000.
    );
    assert!(!studio.busy() && !studio.mix_dirty);
}

#[test]
fn compact_time_inspector_keeps_fractional_fields_and_actions_inside_the_sidebar() {
    for width in [290., 332.] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        // Allow Windows to exercise the installed Linux faces without bundling fonts.
        #[cfg(windows)]
        if let Some(folder) = std::env::var_os("SLIMEYBEATS_TEST_LINUX_FONT_DIR") {
            let folder = std::path::PathBuf::from(folder);
            let mut fonts = egui::FontDefinitions::default();
            for (name, file) in [
                ("system-ui", "DejaVuSans.ttf"),
                ("system-display", "DejaVuSerif.ttf"),
            ] {
                fonts.font_data.insert(
                    name.into(),
                    egui::FontData::from_owned(
                        std::fs::read(folder.join(file)).expect("Installed Linux test font"),
                    )
                    .into(),
                );
            }
            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, "system-ui".into());
            let mut display = vec!["system-display".into()];
            display.extend(
                fonts.families[&egui::FontFamily::Proportional]
                    .iter()
                    .cloned(),
            );
            fonts
                .families
                .insert(egui::FontFamily::Name(theme::DISPLAY_FONT.into()), display);
            ctx.set_fonts(fonts);
        }
        ctx.all_styles_mut(|style| {
            style.animation_time = 0.;
            style.scroll_animation = egui::style::ScrollAnimation::none();
        });
        let mut studio = fixture(100);
        studio.editor.selected = Some(StemKind::Part1);
        studio
            .editor
            .select_time(0.0123456, 0.091234567, 48000, 4800);
        // A user can request the inspector after the window's first layout.
        // Do not consume its one-shot scroll during an initial sizing pass.
        for _ in 0..6 {
            frame(
                &ctx,
                &mut studio,
                false,
                egui::vec2(1100., 720.),
                width,
                vec![],
            );
        }
        studio.editor.inspector_edits = true;
        studio.editor.reveal_edits = true;
        let mut output = None;
        for _ in 0..8 {
            let (next, actual) = frame(
                &ctx,
                &mut studio,
                false,
                egui::vec2(1100., 720.),
                width,
                vec![],
            );
            assert!(
                actual <= width + 0.5,
                "Time fields expanded sidebar: {actual} > {width}"
            );
            output = Some(next);
        }
        let output = output.unwrap();
        for label in [
            "Start",
            "End",
            "Select all",
            "Clear selection",
            "Loop selection",
            "Silence",
            "Fade in",
            "Fade out",
            "Export current preview",
        ] {
            let shape = text(&output, label).unwrap_or_else(|| {
                panic!(
                    "Missing {label} at {width}: {:?}",
                    output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Text(text)
                                if [
                                    "Start",
                                    "End",
                                    "Select all",
                                    "Clear selection",
                                    "Loop selection",
                                    "Silence",
                                    "Fade out"
                                ]
                                .contains(&text.galley.text()) =>
                                Some((
                                    text.galley.text(),
                                    text.visual_bounding_rect(),
                                    shape.clip_rect
                                )),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                )
            });
            assert!(
                shape
                    .clip_rect
                    .expand(1.)
                    .contains_rect(shape.shape.visual_bounding_rect()),
                "Clipped {label}"
            );
        }
        assert_eq!(
            text(&output, "Select all")
                .unwrap()
                .shape
                .visual_bounding_rect()
                .top(),
            text(&output, "Clear selection")
                .unwrap()
                .shape
                .visual_bounding_rect()
                .top(),
            "Selection actions wrapped into another row at {width}"
        );
        assert!(!studio.busy() && !studio.mix_dirty);
    }
}
