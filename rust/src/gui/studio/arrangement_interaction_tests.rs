//! Actual native widgets with synthetic PCM and the private, device-free player fixture.
use super::{
    arrangement_state::LaneInit,
    arrangement_waveforms::PreparedWaveform,
    interaction_tests::{fixture, frame},
    state::{Page, Studio},
};
use crate::{
    audio::Audio,
    gui::theme,
    studio::arrangement::{ArrangementPlayer, Edit},
};
use eframe::egui;
use std::sync::{Arc, atomic::Ordering};

fn tracks() -> Studio {
    let mut studio = fixture(0);
    studio.page = Page::Tracks;
    let mono = Arc::new(Audio::new(vec![0.03; 8000], 8000, 1).unwrap());
    let stereo = Arc::new(Audio::new([0.02, -0.01].repeat(16000), 8000, 2).unwrap());
    studio
        .arrangement
        .editor
        .add_sources_with_controls(vec![
            ("Voice source".into(), mono, LaneInit::default()),
            ("Long backing source".into(), stereo, LaneInit::default()),
        ])
        .unwrap();
    let selected = studio.arrangement.editor.selected_clip.clone().unwrap();
    studio.arrangement.editor.select_clip(&selected).unwrap();
    for (id, audio) in &studio.arrangement.editor.assets {
        studio
            .arrangement
            .waveforms
            .insert(id.clone(), PreparedWaveform::new(audio.clone()).unwrap());
    }
    let player = ArrangementPlayer::new(
        studio.arrangement.editor.document().unwrap(),
        &studio.arrangement.editor.assets,
        &studio.arrangement.options,
    )
    .unwrap();
    let reference = Arc::new(
        crate::studio::arrangement::render(
            studio.arrangement.editor.document().unwrap(),
            &studio.arrangement.editor.assets,
            &studio.arrangement.options,
            &|| false,
        )
        .unwrap(),
    );
    studio
        .player
        .load_arrangement(reference.clone(), player)
        .unwrap();
    studio.arrangement.reference = Some(reference);
    studio.arrangement.sent_epoch = Some(studio.arrangement.editor.epoch);
    studio
}
fn settled(ctx: &egui::Context, studio: &mut Studio, size: egui::Vec2) -> egui::FullOutput {
    let mut output = frame(ctx, studio, false, size, 332., vec![]).0;
    for _ in 0..5 {
        output = frame(ctx, studio, false, size, 332., vec![]).0;
    }
    output
}
fn rect(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    output.shapes.iter().find_map(|item| match &item.shape {
        egui::Shape::Text(text)
            if text.galley.text() == label
                && item.clip_rect.contains_rect(text.visual_bounding_rect()) =>
        {
            Some(text.visual_bounding_rect())
        }
        _ => None,
    })
}
fn click(ctx: &egui::Context, studio: &mut Studio, size: egui::Vec2, label: &str) {
    let output = settled(ctx, studio, size);
    let pos = rect(&output, label)
        .unwrap_or_else(|| panic!("Missing visible action: {label}"))
        .center();
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
    settled(ctx, studio, size);
}

#[test]
fn tracks_have_visible_import_timeline_eq_and_delivery_at_both_desktop_sizes() {
    for size in [egui::vec2(1100., 720.), egui::vec2(1480., 940.)] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut studio = tracks();
        let before = studio.arrangement.editor.document().unwrap().clone();
        let output = settled(&ctx, &mut studio, size);
        for label in [
            "Add tracks…",
            "Add Suno / stem ZIP…",
            "Select time",
            "Move clips",
            "Mix EQ",
            "Export timeline mix",
            "Open project…",
            "Save project…",
            "Play",
            "Stop",
        ] {
            assert!(
                rect(&output, label).is_some(),
                "Clipped/hidden {label} at {size:?}"
            );
        }
        let scroll = rect(&output, "Timeline view").expect("Timeline scrollbar label is visible");
        assert!(
            scroll.bottom() < rect(&output, "Play").unwrap().top(),
            "Timeline view overlaps transport at {size:?}"
        );
        assert_eq!(studio.arrangement.editor.document().unwrap(), &before);
        assert!(!studio.player.controls.playing.load(Ordering::Relaxed));
        assert!(!studio.busy());
    }
}

#[test]
fn real_clip_actions_and_track_order_are_undoable_without_touching_sources() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = tracks();
    let size = egui::vec2(1480., 1800.);
    let original = studio.arrangement.editor.document().unwrap().clone();
    let sources = studio.arrangement.editor.assets.clone();
    let lane = original.lanes[1].id.clone();
    click(&ctx, &mut studio, size, "Duplicate after");
    assert_eq!(
        studio
            .arrangement
            .editor
            .document()
            .unwrap()
            .lane(&lane)
            .unwrap()
            .clips
            .len(),
        2
    );
    click(&ctx, &mut studio, size, "Delete clip");
    assert_eq!(
        studio
            .arrangement
            .editor
            .document()
            .unwrap()
            .lane(&lane)
            .unwrap()
            .clips
            .len(),
        1
    );
    click(&ctx, &mut studio, size, "Undo");
    assert_eq!(
        studio
            .arrangement
            .editor
            .document()
            .unwrap()
            .lane(&lane)
            .unwrap()
            .clips
            .len(),
        2
    );
    click(&ctx, &mut studio, size, "Move track up");
    assert_eq!(
        studio.arrangement.editor.document().unwrap().lanes[0].id,
        lane
    );
    for (id, audio) in sources {
        assert!(Arc::ptr_eq(&audio, &studio.arrangement.editor.assets[&id]));
    }
    assert!(!studio.player.controls.playing.load(Ordering::Relaxed));
}

#[test]
fn section_eq_widget_changes_only_the_selected_scope_and_can_be_bypassed() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = tracks();
    let size = egui::vec2(1480., 1800.);
    click(&ctx, &mut studio, size, "Mix EQ");
    click(&ctx, &mut studio, size, "Only selected time range");
    click(&ctx, &mut studio, size, "Bypass this EQ");
    let effect = studio.arrangement.editor.document().unwrap().eq[0].clone();
    assert_eq!(effect.scope, crate::studio::arrangement::EqScope::Mix {});
    assert!(effect.range.is_some());
    assert!(effect.bypass);
    click(&ctx, &mut studio, size, "Bypass this EQ");
    assert_eq!(studio.arrangement.editor.document().unwrap().eq.len(), 1);
    assert!(!studio.arrangement.editor.document().unwrap().eq[0].bypass);
    click(&ctx, &mut studio, size, "Undo");
    assert!(studio.arrangement.editor.document().unwrap().eq[0].bypass);
}

#[test]
fn changed_timeline_blocks_stale_legacy_preview_export_and_tools() {
    let mut studio = tracks();
    let source = studio.source.as_ref().unwrap().audio.clone();
    studio.arrangement.rendered_audio = Some(source);
    studio.arrangement.rendered_epoch = Some(studio.arrangement.editor.epoch);
    assert!(studio.editing_target().is_ok());
    let clip = studio.arrangement.editor.document().unwrap().lanes[0].clips[0]
        .id
        .clone();
    studio
        .arrangement
        .editor
        .apply(Edit::SetClipGain {
            clip_id: clip,
            gain_db: -3.,
        })
        .unwrap();
    assert!(studio.editing_target().is_err());
    assert!(
        super::preview_actions::preview_unavailable(&studio)
            .unwrap()
            .contains("Timeline changed")
    );
    studio.export_preview(&egui::Context::default());
    assert!(
        studio
            .error
            .as_deref()
            .unwrap()
            .contains("timeline changed")
    );
    assert!(!studio.busy());
}

#[test]
fn painting_an_in_progress_track_name_preserves_the_draft_until_selection_changes() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = tracks();
    let lane = studio.arrangement.editor.selected_lane.clone().unwrap();
    let previous = studio
        .arrangement
        .editor
        .document()
        .unwrap()
        .lane(&lane)
        .unwrap()
        .name
        .clone();
    studio.arrangement.editor.lane_name_draft = Some((lane.clone(), "My partly typed name".into()));
    let output = settled(&ctx, &mut studio, egui::vec2(1480., 1800.));
    assert!(rect(&output, "My partly typed name").is_some());
    assert_eq!(
        studio
            .arrangement
            .editor
            .document()
            .unwrap()
            .lane(&lane)
            .unwrap()
            .name,
        previous
    );
    let next = studio.arrangement.editor.document().unwrap().lanes[0].clips[0]
        .id
        .clone();
    studio.arrangement.editor.select_clip(&next).unwrap();
    assert_eq!(
        studio
            .arrangement
            .editor
            .document()
            .unwrap()
            .lane(&lane)
            .unwrap()
            .name,
        "My partly typed name"
    );
}

#[test]
fn compact_clip_fields_are_reachable_by_scrolling_without_moving_delivery_actions() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = tracks();
    let size = egui::vec2(1100., 720.);
    let original = studio.arrangement.editor.document().unwrap().clone();
    let first = settled(&ctx, &mut studio, size);
    let delivery = rect(&first, "Export timeline mix").unwrap();
    let mut found = std::collections::BTreeSet::new();
    for _ in 0..24 {
        let output = frame(
            &ctx,
            &mut studio,
            false,
            size,
            332.,
            vec![
                egui::Event::PointerMoved(egui::pos2(size.x - 180., 330.)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., -80.),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
        .0;
        for label in [
            "Position",
            "Source in",
            "Source out",
            "Clip level",
            "Fade in",
            "Fade out",
            "Duplicate after",
            "Delete clip",
        ] {
            if rect(&output, label).is_some() {
                found.insert(label);
            }
        }
        assert_eq!(rect(&output, "Export timeline mix").unwrap(), delivery);
    }
    assert_eq!(found.len(), 8, "Unreachable clip controls: {found:?}");
    assert_eq!(studio.arrangement.editor.document().unwrap(), &original);
}
