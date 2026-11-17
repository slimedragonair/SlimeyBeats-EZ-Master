//! Real native tab widgets and prepared players, without opening an audio device.
use super::{
    arrangement_loading::{self, ImportContext},
    arrangement_runtime::Change,
    arrangement_state::LaneInit,
    interaction_tests::{fixture, frame},
    state::{Page, Studio},
};
use crate::{audio::Audio, gui::theme};
use eframe::egui;
use std::{
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    ctx.all_styles_mut(|style| style.animation_time = 0.);
    ctx
}

fn ready_session(studio: &Studio) -> super::arrangement_runtime::PreparedSession {
    arrangement_loading::prepare_named(
        ImportContext {
            editor: Default::default(),
            expected_epoch: studio.arrangement.editor.epoch,
            reference: None,
            origin: None,
            options: studio.arrangement.options.clone(),
        },
        vec![(
            "Timeline only".into(),
            Arc::new(Audio::new(vec![0.075; 16000], 8000, 1).unwrap()),
            LaneInit::default(),
        )],
        &|| false,
        &|_, _| {},
    )
    .unwrap()
}

fn studio_with_both_workspaces(ctx: &egui::Context, parts: u32) -> Studio {
    let mut studio = fixture(parts);
    if parts == 0 {
        let current = studio.current.as_ref().unwrap();
        studio
            .player
            .load(
                current.before.clone(),
                Some(current.audio.clone()),
                -20.,
                -20.,
                None,
            )
            .unwrap();
    }
    let ready = ready_session(&studio);
    studio.accept_arrangement(Change::Session(Box::new(ready)), ctx);
    studio
}

fn paint(ctx: &egui::Context, studio: &mut Studio, size: egui::Vec2) -> egui::FullOutput {
    let mut output = frame(ctx, studio, false, size, 332., vec![]).0;
    for _ in 0..4 {
        output = frame(ctx, studio, false, size, 332., vec![]).0;
    }
    output
}

fn click(ctx: &egui::Context, studio: &mut Studio, size: egui::Vec2, label: &str) {
    let output = paint(ctx, studio, size);
    let pos = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text)
                if text.galley.text() == label
                    && shape.clip_rect.contains_rect(text.visual_bounding_rect()) =>
            {
                Some(text.visual_bounding_rect().center())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing visible native tab: {label}"));
    for pressed in [true, false] {
        frame(
            ctx,
            studio,
            false,
            size,
            332.,
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
}

fn finish(ctx: &egui::Context, studio: &mut Studio) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while studio.busy() && Instant::now() < deadline {
        studio.poll(ctx);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!studio.busy(), "Playback preparation did not finish");
    assert!(studio.error.is_none(), "{:?}", studio.error);
}

#[test]
fn native_tab_roundtrip_preserves_both_positions_sides_and_pending_live_mix() {
    for size in [egui::vec2(1100., 720.), egui::vec2(1480., 1800.)] {
        for page in [Page::Stems, Page::Voice, Page::Repair, Page::Plugins] {
            let ctx = context();
            let mut studio = studio_with_both_workspaces(&ctx, 2);
            let name = match page {
                Page::Stems => {
                    if size.y < 800. {
                        "Stems"
                    } else {
                        "Stem Studio"
                    }
                }
                Page::Voice => {
                    if size.y < 800. {
                        "Voice"
                    } else {
                        "Voice cleanup"
                    }
                }
                Page::Repair => {
                    if size.y < 800. {
                        "Repair"
                    } else {
                        "Spectral repair"
                    }
                }
                Page::Plugins => "Plugins",
                Page::Tracks => unreachable!(),
            };
            studio.player.seek(1.25);
            studio.player.controls.after.store(false, Ordering::Relaxed);
            studio
                .player
                .controls
                .playing
                .store(true, Ordering::Relaxed);
            click(&ctx, &mut studio, size, name);
            assert!(
                !studio.player.has_live_arrangement(),
                "{name} retained timeline playback"
            );
            assert!(studio.player.has_live_mix());
            assert!(!studio.player.controls.playing.load(Ordering::Relaxed));
            studio.player.seek(0.035);
            studio.mix.tracks[0].gain_db = -7.125;
            studio.mix.tracks[1].mute = true;
            studio.monitor_mix_changes();
            studio.match_levels = true;
            studio.undo.push(studio.current.clone().unwrap());
            studio.redo.push(studio.current.clone().unwrap());
            let source = studio.source.as_ref().unwrap().audio.clone();
            let preview = studio.current.as_ref().unwrap().audio.clone();
            let bank = studio.bank.clone().unwrap();
            let assets = studio.arrangement.editor.assets.clone();
            let source_bits = source
                .samples
                .iter()
                .map(|x| x.to_bits())
                .collect::<Vec<_>>();
            let settings =
                serde_json::to_value((&studio.mix, &studio.voice, &studio.repair)).unwrap();
            let history =
                serde_json::to_value(studio.arrangement.snapshot().unwrap().history).unwrap();
            let legacy_history = (studio.undo.len(), studio.redo.len());
            let image =
                Arc::new(crate::studio::spectrogram::build(&source, 16, 16, &|| false).unwrap());
            studio.spectrogram = Some(image.clone());
            studio
                .player
                .controls
                .playing
                .store(true, Ordering::Relaxed);
            click(&ctx, &mut studio, size, "Tracks");
            assert!(!studio.player.controls.playing.load(Ordering::Relaxed));
            finish(&ctx, &mut studio);
            assert!(studio.player.has_live_arrangement() && !studio.player.has_live_mix());
            assert_eq!(studio.player.duration, 2.);
            assert_eq!(studio.player.seconds(), 1.25);
            assert!(!studio.player.controls.after.load(Ordering::Relaxed));
            studio
                .player
                .controls
                .playing
                .store(true, Ordering::Relaxed);
            click(&ctx, &mut studio, size, name);
            assert!(!studio.player.has_live_arrangement() && studio.player.has_live_mix());
            assert_eq!(studio.player.duration, 0.1);
            assert_eq!(studio.player.seconds(), 0.035);
            assert!(!studio.player.controls.playing.load(Ordering::Relaxed));
            assert!(studio.player.controls.after.load(Ordering::Relaxed));
            assert!(studio.match_levels);
            assert!(!studio.player.controls.matched.load(Ordering::Relaxed));
            assert!(studio.mix_dirty && studio.monitoring_mix);
            let targets = studio
                .player
                .restored_mix_mailbox()
                .unwrap()
                .take()
                .expect("Pending live mix was not restored to the new stem engine");
            let mut actual = crate::studio::realtime_mix::RealtimeMix::new(
                bank.clone(),
                &studio.current.as_ref().unwrap().mix,
            )
            .unwrap();
            actual.set_targets(targets);
            actual.reset();
            let mut expected =
                crate::studio::realtime_mix::RealtimeMix::new(bank.clone(), &studio.mix).unwrap();
            for frame in 0..bank.tracks[0].audio.frames() {
                assert_eq!(actual.process(frame), expected.process(frame));
            }
            assert!(Arc::ptr_eq(&image, studio.spectrogram.as_ref().unwrap()));
            assert!(Arc::ptr_eq(
                &preview,
                &studio.current.as_ref().unwrap().audio
            ));
            assert!(Arc::ptr_eq(&source, &studio.source.as_ref().unwrap().audio));
            assert!(Arc::ptr_eq(&bank, studio.bank.as_ref().unwrap()));
            for (id, audio) in &assets {
                assert!(Arc::ptr_eq(audio, &studio.arrangement.editor.assets[id]));
            }
            assert_eq!(
                source_bits,
                source
                    .samples
                    .iter()
                    .map(|x| x.to_bits())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                settings,
                serde_json::to_value((&studio.mix, &studio.voice, &studio.repair)).unwrap()
            );
            assert_eq!(
                history,
                serde_json::to_value(studio.arrangement.snapshot().unwrap().history).unwrap()
            );
            assert_eq!(legacy_history, (studio.undo.len(), studio.redo.len()));
        }
    }
}

#[test]
fn legacy_tabs_keep_running_player_and_doctor_also_releases_tracks() {
    let ctx = context();
    let size = egui::vec2(1100., 720.);
    let mut studio = studio_with_both_workspaces(&ctx, 2);
    click(&ctx, &mut studio, size, "Doctor");
    assert_eq!(studio.page, Page::Repair);
    assert!(studio.doctor.visible);
    assert!(!studio.player.has_live_arrangement() && studio.player.has_live_mix());
    studio.player.seek(0.045);
    studio.player.controls.after.store(false, Ordering::Relaxed);
    studio
        .player
        .controls
        .playing
        .store(true, Ordering::Relaxed);
    for name in ["Voice", "Stems", "Plugins", "Repair", "Doctor"] {
        click(&ctx, &mut studio, size, name);
        assert!(studio.player.has_live_mix() && !studio.player.has_live_arrangement());
        assert_eq!(studio.player.seconds(), 0.045);
        assert!(!studio.player.controls.after.load(Ordering::Relaxed));
        assert!(studio.player.controls.playing.load(Ordering::Relaxed));
        assert!(!studio.busy());
    }
}

#[test]
fn leaving_tracks_without_a_legacy_preview_unloads_the_timeline() {
    let ctx = context();
    let mut studio = studio_with_both_workspaces(&ctx, 0);
    studio.current = None;
    studio.source = None;
    studio
        .player
        .controls
        .playing
        .store(true, Ordering::Relaxed);
    click(&ctx, &mut studio, egui::vec2(1100., 720.), "Voice");
    assert_eq!(studio.page, Page::Voice);
    assert!(!studio.player.has_live_arrangement() && !studio.player.has_live_mix());
    assert_eq!(studio.player.duration, 0.);
    assert!(!studio.player.controls.playing.load(Ordering::Relaxed));
    assert!(!studio.busy());
}

#[test]
fn session_import_remembers_legacy_position_even_when_tracks_page_is_already_selected() {
    let ctx = context();
    let mut studio = fixture(2);
    studio.page = Page::Tracks;
    studio.player.seek(0.055);
    studio.player.controls.after.store(false, Ordering::Relaxed);
    let ready = ready_session(&studio);
    studio.accept_arrangement(Change::Session(Box::new(ready)), &ctx);
    assert!(studio.player.has_live_arrangement());
    click(&ctx, &mut studio, egui::vec2(1100., 720.), "Stems");
    assert!(studio.player.has_live_mix());
    assert_eq!(studio.player.seconds(), 0.055);
    assert!(!studio.player.controls.after.load(Ordering::Relaxed));
}

#[test]
fn player_result_remembers_legacy_position_even_when_tracks_page_is_already_selected() {
    let ctx = context();
    let mut studio = fixture(2);
    let ready = ready_session(&studio);
    studio.arrangement.editor = ready.editor;
    studio.page = Page::Tracks;
    studio.player.seek(0.065);
    studio.player.controls.after.store(false, Ordering::Relaxed);
    studio.accept_arrangement(
        Change::Player {
            epoch: studio.arrangement.editor.epoch,
            player: Box::new(ready.player),
            reference: ready.reference,
        },
        &ctx,
    );
    assert!(studio.player.has_live_arrangement());
    click(&ctx, &mut studio, egui::vec2(1100., 720.), "Voice");
    assert!(studio.player.has_live_mix());
    assert_eq!(studio.player.seconds(), 0.065);
    assert!(!studio.player.controls.after.load(Ordering::Relaxed));
}

#[test]
fn a_plain_legacy_preview_returns_without_creating_a_stem_engine() {
    let ctx = context();
    let mut studio = studio_with_both_workspaces(&ctx, 0);
    click(&ctx, &mut studio, egui::vec2(1100., 720.), "Voice");
    assert!(!studio.player.has_live_arrangement() && !studio.player.has_live_mix());
    assert_eq!(studio.player.duration, 0.1);
    assert!(!studio.player.controls.playing.load(Ordering::Relaxed));
}

#[test]
fn empty_tracks_never_leaves_legacy_audio_under_its_disabled_transport() {
    let ctx = context();
    let mut studio = fixture(2);
    studio.player.seek(0.075);
    studio.player.controls.after.store(false, Ordering::Relaxed);
    studio
        .player
        .controls
        .playing
        .store(true, Ordering::Relaxed);
    let size = egui::vec2(1100., 720.);
    click(&ctx, &mut studio, size, "Tracks");
    assert_eq!(studio.page, Page::Tracks);
    assert!(!studio.player.has_live_arrangement() && !studio.player.has_live_mix());
    assert_eq!(studio.player.duration, 0.);
    assert!(!studio.player.controls.playing.load(Ordering::Relaxed));
    assert!(!studio.busy());
    click(&ctx, &mut studio, size, "Voice");
    assert!(studio.player.has_live_mix());
    assert_eq!(studio.player.seconds(), 0.075);
    assert!(!studio.player.controls.after.load(Ordering::Relaxed));
    assert!(!studio.player.controls.playing.load(Ordering::Relaxed));
}

#[test]
fn late_tracks_preparation_cannot_replace_the_restored_legacy_player() {
    let ctx = context();
    let mut studio = fixture(2);
    let ready = ready_session(&studio);
    studio.arrangement.editor = ready.editor;
    studio.arrangement.reference = Some(ready.reference);
    studio.player.seek(0.085);
    studio.player.controls.after.store(false, Ordering::Relaxed);
    studio.activate_page(Page::Tracks, &ctx);
    assert!(studio.busy());
    studio.activate_page(Page::Voice, &ctx);
    let deadline = Instant::now() + Duration::from_secs(5);
    while studio.busy() && Instant::now() < deadline {
        studio.poll(&ctx);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!studio.busy());
    assert!(
        studio
            .error
            .as_deref()
            .unwrap()
            .contains("older timeline or workspace")
    );
    assert_eq!(studio.page, Page::Voice);
    assert!(studio.player.has_live_mix() && !studio.player.has_live_arrangement());
    assert_eq!(studio.player.seconds(), 0.085);
    assert!(!studio.player.controls.after.load(Ordering::Relaxed));
    assert!(!studio.player.controls.playing.load(Ordering::Relaxed));
}
