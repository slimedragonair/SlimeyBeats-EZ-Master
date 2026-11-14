//! Headless native-widget checks; these never open an audio output device.
use super::{
    controls,
    state::{LoadedSource, Page, Revision, Studio, Target},
    workspace,
};
use crate::{
    audio::Audio,
    gui::theme,
    studio::{
        spectral_repair::RepairMode,
        stem_mix::{Stem, StemKind, StemSet},
        voice_cleanup::NoiseRegion,
    },
};
use eframe::egui;
use std::sync::Arc;

fn fixture(parts: u32) -> Studio {
    let mut studio = Studio::new("F:/Exports/Long project folder/Studio mixes".into());
    studio.player = crate::gui::player::Player::for_processing_tests();
    studio.plugin_host = None;
    let audio = Arc::new(Audio::new(vec![0.02; 9600], 48000, 2).unwrap());
    studio.source = Some(LoadedSource {
        audio: audio.clone(),
        path: "Synthetic test mix.wav".into(),
        loudness: -20.,
    });
    if parts > 0 {
        let mut bank = StemSet::new(
            (1..=parts)
                .map(|index| Stem {
                    kind: StemKind::part(index).unwrap(),
                    audio: audio.clone(),
                })
                .collect(),
        )
        .unwrap();
        for index in 1..=parts {
            bank.set_display_name(
                StemKind::part(index).unwrap(),
                format!("Part {index:03} · {}", "Long instrument title ".repeat(5)),
            )
            .unwrap();
        }
        studio.mix = bank.defaults();
        studio.stem_waveforms = bank
            .tracks
            .iter()
            .map(|stem| {
                (
                    stem.kind,
                    crate::gui::plots::Waveform::new(&stem.audio, None),
                )
            })
            .collect();
        studio.bank = Some(Arc::new(bank));
    }
    studio.current = Some(Revision {
        before: audio.clone(),
        audio,
        label: "Synthetic preview".into(),
        notes: vec![],
        before_lufs: -20.,
        after_lufs: -20.,
        bank: studio.bank.clone(),
        mix: studio.mix.clone(),
    });
    studio
}

fn frame(
    ctx: &egui::Context,
    studio: &mut Studio,
    advanced: bool,
    size: egui::Vec2,
    panel_width: f32,
    events: Vec<egui::Event>,
) -> (egui::FullOutput, f32) {
    let mut width = 0.;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| {
            // Reserve the actual shell's header/footer space without constructing its player.
            let rect = ui.max_rect();
            let mut content =
                ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_max(
                    rect.min + egui::vec2(0., 90.),
                    rect.max - egui::vec2(0., 70.),
                )));
            width = egui::Panel::right("studio-test-settings")
                .default_size(panel_width)
                .min_size(290.)
                .max_size(450.)
                .resizable(true)
                .show(&mut content, |ui| {
                    egui::Frame::new().inner_margin(12).show(ui, |ui| {
                        ui.add_space(38.); // Parent's Basic/Advanced selector.
                        controls::show(studio, ui, None, ctx, advanced);
                    });
                })
                .response
                .rect
                .width();
            egui::CentralPanel::default().show(&mut content, |ui| {
                egui::Frame::new().inner_margin(12).show(ui, |ui| {
                    ui.add_space(86.); // Parent's current-source title and main tabs.
                    workspace::show(studio, ui, ctx, advanced)
                });
            });
        },
    );
    output.textures_delta.clear();
    (output, width)
}

fn text<'a>(output: &'a egui::FullOutput, needle: &str) -> Option<&'a egui::epaint::ClippedShape> {
    output.shapes.iter().find(|shape| match &shape.shape {
        egui::Shape::Text(label) => {
            label.galley.text().contains(needle)
                && shape.clip_rect.intersects(label.visual_bounding_rect())
        }
        _ => false,
    })
}

fn click(
    ctx: &egui::Context,
    studio: &mut Studio,
    output: &egui::FullOutput,
    needle: &str,
) -> egui::FullOutput {
    let pos = text(output, needle)
        .unwrap_or_else(|| panic!("Missing visible control: {needle}"))
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
        egui::vec2(1480., 1800.),
        332.,
        vec![egui::Event::PointerMoved(pos), event(true)],
    );
    frame(
        ctx,
        studio,
        false,
        egui::vec2(1480., 1800.),
        332.,
        vec![event(false)],
    )
    .0
}

#[test]
fn basic_and_page_changes_preserve_custom_settings_and_never_start_jobs() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = fixture(2);
    studio.voice.highpass_hz = 110.;
    studio.voice.noise_region = Some(NoiseRegion {
        start_seconds: 0.01,
        end_seconds: 0.05,
    });
    studio.voice.gate_threshold_db = Some(-48.);
    studio.voice.output_gain_db = 2.;
    studio.voice.protect_peaks = false;
    studio.voice.hold_loudness = false;
    studio.repair.mode = RepairMode::Heal;
    studio.mix.tracks[0].balance = 0.37;
    studio.mix.tracks[0].gain_db = -4.;
    studio.mix_dirty = true;
    let voice = serde_json::to_value(&studio.voice).unwrap();
    let repair = serde_json::to_value(&studio.repair).unwrap();
    let mix = serde_json::to_value(&studio.mix).unwrap();
    let audio = studio.current.as_ref().unwrap().audio.clone();
    for advanced in [false, true, false] {
        for page in [Page::Voice, Page::Repair, Page::Plugins, Page::Stems] {
            studio.page = page;
            for _ in 0..4 {
                frame(
                    &ctx,
                    &mut studio,
                    advanced,
                    egui::vec2(1100., 720.),
                    290.,
                    vec![],
                );
            }
            assert_eq!(serde_json::to_value(&studio.voice).unwrap(), voice);
            assert_eq!(serde_json::to_value(&studio.repair).unwrap(), repair);
            assert_eq!(serde_json::to_value(&studio.mix).unwrap(), mix);
            assert!(studio.mix_dirty);
            assert!(
                !studio.busy(),
                "Changing disclosure must never process or download"
            );
            assert!(Arc::ptr_eq(&audio, &studio.current.as_ref().unwrap().audio));
        }
    }
}

#[test]
fn basic_shows_custom_hints_and_keeps_repair_numeric_fields_accessible() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = fixture(2);
    studio.page = Page::Voice;
    studio.voice.gate_threshold_db = Some(-48.);
    let (output, _) = frame(
        &ctx,
        &mut studio,
        false,
        egui::vec2(1480., 1800.),
        332.,
        vec![],
    );
    assert!(text(&output, "Custom voice settings active").is_some());
    assert!(text(&output, "Quiet-gap gate").is_none());
    assert!(text(&output, "Use noise-only region").is_none());
    studio.page = Page::Repair;
    studio.repair.mode = RepairMode::Heal;
    let (output, _) = frame(
        &ctx,
        &mut studio,
        false,
        egui::vec2(1480., 1800.),
        332.,
        vec![],
    );
    for label in [
        "Custom repair mode active",
        "Start",
        "End",
        "Low",
        "High",
        "Reduction",
        "Preview spectral repair",
    ] {
        assert!(text(&output, label).is_some(), "{label}");
    }
    assert!(text(&output, "Turn down selected sound").is_none());
}

#[test]
fn hundred_long_named_parts_do_not_expand_panel_or_hide_pinned_exports() {
    for (size, width) in [
        (egui::vec2(1100., 720.), 290.),
        (egui::vec2(1480., 940.), 332.),
    ] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut studio = fixture(100);
        let mut output = None;
        for advanced in [false, true, false] {
            for _ in 0..16 {
                let (next, actual) = frame(&ctx, &mut studio, advanced, size, width, vec![]);
                assert!(
                    actual <= width + 0.5,
                    "Panel expanded to {actual} from {width}"
                );
                output = Some(next);
            }
            let output = output.as_ref().unwrap();
            for label in [
                "Export current preview",
                "Export raw stems",
                "Format",
                "Folder…",
                "Save project…",
                "Update mix preview",
                "Mute",
                "Solo",
            ] {
                let shape =
                    text(output, label).unwrap_or_else(|| panic!("Hidden {label} at {size:?}"));
                assert!(
                    shape
                        .clip_rect
                        .expand(1.)
                        .contains_rect(shape.shape.visual_bounding_rect()),
                    "Clipped {label} at {size:?}"
                );
            }
        }
        assert_eq!(studio.bank.as_ref().unwrap().tracks.len(), 100);
    }
}

#[test]
fn pending_faders_disable_preview_actions_but_leave_read_only_input_available() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = fixture(2);
    studio.mix.tracks[0].gain_db = -6.;
    studio.mix_dirty = true;
    assert!(
        controls::preview_unavailable(&studio)
            .unwrap()
            .contains("Update mix preview")
    );
    for (page, action) in [
        (Page::Voice, "Preview voice cleanup"),
        (Page::Repair, "Preview spectral repair"),
    ] {
        studio.page = page;
        let (output, _) = frame(
            &ctx,
            &mut studio,
            false,
            egui::vec2(1480., 1800.),
            332.,
            vec![],
        );
        click(&ctx, &mut studio, &output, action);
        assert!(!studio.busy());
        assert!(studio.audio_target().is_ok());
        assert!(studio.editing_target().is_err());
        assert_eq!(studio.mix.tracks[0].gain_db, -6.);
        assert!(studio.mix_dirty);
    }
    studio.jobs.active = Some(1);
    assert!(
        controls::preview_unavailable(&studio)
            .unwrap()
            .contains("current job")
    );
}

#[test]
fn mute_checkbox_accepts_keyboard_activation_and_marks_mix_pending() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = fixture(2);
    frame(
        &ctx,
        &mut studio,
        false,
        egui::vec2(1480., 1800.),
        332.,
        vec![],
    );
    let key = |key, pressed| egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let mut focused_mute = None;
    for _ in 0..64 {
        let output = frame(
            &ctx,
            &mut studio,
            false,
            egui::vec2(1480., 1800.),
            332.,
            vec![key(egui::Key::Tab, true)],
        )
        .0;
        frame(
            &ctx,
            &mut studio,
            false,
            egui::vec2(1480., 1800.),
            332.,
            vec![key(egui::Key::Tab, false)],
        );
        let mute_rect = text(&output, "Mute").unwrap().shape.visual_bounding_rect();
        if let Some(id) = ctx.memory(|memory| memory.focused())
            && ctx
                .read_response(id)
                .is_some_and(|response| response.rect.intersects(mute_rect))
        {
            focused_mute = Some(id);
            break;
        }
    }
    let focused_mute = focused_mute.expect("Mute must be reachable using Tab");
    frame(
        &ctx,
        &mut studio,
        false,
        egui::vec2(1480., 1800.),
        332.,
        vec![key(egui::Key::Space, true)],
    );
    assert!(studio.mix.tracks[0].mute);
    frame(
        &ctx,
        &mut studio,
        false,
        egui::vec2(1480., 1800.),
        332.,
        vec![key(egui::Key::Space, false)],
    );
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(focused_mute),
        "Pending notice must preserve the focused stem control"
    );
    frame(
        &ctx,
        &mut studio,
        false,
        egui::vec2(1480., 1800.),
        332.,
        vec![key(egui::Key::Space, true)],
    );
    assert!(!studio.mix.tracks[0].mute);
    assert!(studio.mix_dirty);
}

#[test]
fn scrolling_a_hundred_part_bank_reaches_the_last_part_and_keeps_export_visible() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = fixture(100);
    let size = egui::vec2(1100., 720.);
    let mut output = frame(&ctx, &mut studio, false, size, 290., vec![]).0;
    for _ in 0..6 {
        output = frame(&ctx, &mut studio, false, size, 290., vec![]).0;
    }
    let pointer = text(&output, "Part 001")
        .unwrap()
        .shape
        .visual_bounding_rect()
        .center();
    let mut reached_last = false;
    for _ in 0..80 {
        output = frame(
            &ctx,
            &mut studio,
            false,
            size,
            290.,
            vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., -1200.),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
        .0;
        assert!(text(&output, "Export current preview").is_some());
        if text(&output, "Part 100").is_some() {
            reached_last = true;
            break;
        }
    }
    assert!(
        reached_last,
        "The final stem must be reachable by scrolling the fader list"
    );
    assert!(!studio.busy());
    assert!(!studio.mix_dirty);
}

#[test]
fn selected_stem_apply_stays_blocked_when_faders_are_pending() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = fixture(2);
    studio.page = Page::Voice;
    studio.target = Target::Stem(StemKind::Part1);
    assert!(studio.can_apply_preview_to_stem());
    studio.mix_dirty = true;
    let bank = studio.bank.clone().unwrap();
    let (output, _) = frame(
        &ctx,
        &mut studio,
        false,
        egui::vec2(1480., 1800.),
        332.,
        vec![],
    );
    click(&ctx, &mut studio, &output, "Apply preview to selected stem");
    assert!(!studio.busy());
    assert!(Arc::ptr_eq(&bank, studio.bank.as_ref().unwrap()));
    assert!(studio.mix_dirty);
}
