//! Real egui pointer events through the production timeline and applied-effect widgets.
use super::super::{
    arrangement_eq_effects,
    arrangement_state::{ArrangementEditor, EqTarget},
    arrangement_timeline,
    arrangement_waveforms::Waveforms,
};
use super::{Gesture, Kind};
use crate::{
    audio::Audio,
    gui::theme,
    studio::arrangement::{Edit, EqScope, FrameRange, Lane, ScopedEq},
};
use eframe::egui::{self, Color32, Pos2, Rect};
use std::sync::Arc;

struct Harness {
    ctx: egui::Context,
    editor: ArrangementEditor,
    gesture: Option<Gesture>,
    effects: bool,
}
impl Harness {
    fn new() -> Self {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut editor = ArrangementEditor::default();
        editor
            .add_sources_with_controls(vec![
                (
                    "Gesture first".into(),
                    Arc::new(Audio::new(vec![0.03; 8000], 8000, 1).unwrap()),
                    Default::default(),
                ),
                (
                    "Gesture second".into(),
                    Arc::new(Audio::new(vec![0.01; 16000], 8000, 1).unwrap()),
                    Default::default(),
                ),
            ])
            .unwrap();
        let first = editor.document().unwrap().lanes[0].clone();
        editor
            .apply(Edit::MoveClip {
                clip_id: first.clips[0].id.clone(),
                lane_id: first.id,
                start_frame: 2000,
            })
            .unwrap();
        let second = editor.document().unwrap().lanes[1].clips[0].id.clone();
        editor.select_clip(&second).unwrap();
        Self {
            ctx,
            editor,
            gesture: None,
            effects: false,
        }
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000., 720.))),
                events,
                ..Default::default()
            },
            |ui| {
                if self.effects {
                    arrangement_eq_effects::show(&mut self.editor, ui).unwrap();
                } else {
                    arrangement_timeline::show(
                        &mut self.editor,
                        &Waveforms::default(),
                        &mut self.gesture,
                        ui,
                        0.,
                    )
                    .unwrap();
                }
            },
        );
        output.textures_delta.clear();
        output
    }
    fn settled(&mut self) -> egui::FullOutput {
        let mut output = self.frame(vec![]);
        for _ in 0..5 {
            output = self.frame(vec![]);
        }
        output
    }
    fn press(&mut self, pos: Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos), button(pos, true)]);
    }
    fn motion(&mut self, pos: Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos)]);
    }
    fn release(&mut self, pos: Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos), button(pos, false)]);
    }
    fn click(&mut self, label: &str, index: usize) {
        let output = self.settled();
        let pos = output
            .shapes
            .iter()
            .filter_map(|item| match &item.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .nth(index)
            .unwrap_or_else(|| panic!("Missing action {label} #{index}"));
        self.press(pos);
        self.release(pos);
        self.settled();
    }
    fn escape(&mut self) {
        self.frame(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
    fn clips(&mut self) -> Vec<Rect> {
        self.settled()
            .shapes
            .iter()
            .filter_map(|item| match &item.shape {
                egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(80, 45, 112) => {
                    Some(rect.rect)
                }
                _ => None,
            })
            .collect()
    }
    fn first(&self) -> crate::studio::arrangement::Clip {
        self.editor
            .document()
            .unwrap()
            .lanes
            .iter()
            .flat_map(|lane| &lane.clips)
            .find(|clip| clip.source_id == "source_1")
            .unwrap()
            .clone()
    }
}
fn button(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}
fn section(id: &str, scope: EqScope, range: Option<FrameRange>) -> ScopedEq {
    let mut bands = crate::config::default_eq();
    bands[2].gain = -2.;
    ScopedEq {
        id: id.into(),
        scope,
        range,
        bands,
        output_gain_db: -1.5,
        bypass: false,
    }
}

#[test]
fn first_click_title_drag_applies_the_initial_motion_and_commits_one_undo_step() {
    let mut h = Harness::new();
    let before = h.first();
    let original = h.editor.document().unwrap().clone();
    let history = h.editor.history.as_ref().unwrap().undo_len();
    let rect = h.clips()[0];
    let press = egui::pos2(rect.left() + 30., rect.top() + 10.);
    let moved = press + egui::vec2(12., 0.);
    let frames = (12. * before.duration_frames() as f64 / rect.width() as f64).round() as u64;
    h.press(press);
    h.motion(moved);
    assert!(matches!(h.gesture.as_ref().unwrap().kind, Kind::Move));
    assert_eq!(h.first().start_frame, before.start_frame + frames);
    assert_eq!(h.editor.history.as_ref().unwrap().current(), &original);
    h.release(moved);
    assert!(h.gesture.is_none());
    assert_eq!(h.editor.history.as_ref().unwrap().undo_len(), history + 1);
    h.editor.history_step(false).unwrap();
    assert_eq!(h.editor.document().unwrap(), &original);
}

#[test]
fn first_click_both_trim_edges_remain_trims_after_crossing_the_drag_threshold() {
    for left in [true, false] {
        let mut h = Harness::new();
        let before = h.first();
        let rect = h.clips()[0];
        let press = egui::pos2(
            if left {
                rect.left() + 4.
            } else {
                rect.right() - 4.
            },
            rect.center().y,
        );
        let moved = press + egui::vec2(if left { 12. } else { -12. }, 0.);
        let frames = (12. * before.duration_frames() as f64 / rect.width() as f64).round() as u64;
        h.press(press);
        h.motion(moved);
        if left {
            assert!(matches!(h.gesture.as_ref().unwrap().kind, Kind::TrimStart));
            assert_eq!(h.first().start_frame, before.start_frame + frames);
            assert_eq!(
                h.first().source_start_frame,
                before.source_start_frame + frames
            );
        } else {
            assert!(matches!(h.gesture.as_ref().unwrap().kind, Kind::TrimEnd));
            assert_eq!(h.first().end_frame, before.end_frame - frames);
            assert_eq!(h.first().source_start_frame, before.source_start_frame);
        }
        h.release(moved);
        assert!(h.gesture.is_none());
    }
}

#[test]
fn waveform_selection_starts_at_the_actual_press_without_losing_initial_motion() {
    let mut h = Harness::new();
    let clip = h.first();
    let rect = h.clips()[0];
    let press = egui::pos2(rect.left() + rect.width() * 0.3, rect.center().y);
    let moved = press + egui::vec2(20., 0.);
    let first = clip.start_frame + (clip.duration_frames() as f64 * 0.3).round() as u64;
    let last = first + (20. * clip.duration_frames() as f64 / rect.width() as f64).round() as u64;
    h.press(press);
    h.motion(moved);
    assert!(matches!(h.gesture.as_ref().unwrap().kind, Kind::Select));
    let range = h.editor.selection.unwrap();
    assert!(range.start_frame.abs_diff(first) <= 1, "{range:?}");
    assert!(range.end_frame.abs_diff(last) <= 1, "{range:?}");
    h.release(moved);
    assert_eq!(h.first(), clip);
}

#[test]
fn cross_lane_drag_keeps_the_last_valid_lane_when_released_outside_the_viewport() {
    let mut h = Harness::new();
    let before = h.first();
    let sources = h.editor.assets.clone();
    let target = h.editor.document().unwrap().lanes[1].id.clone();
    let rects = h.clips();
    let press = egui::pos2(rects[0].left() + 30., rects[0].top() + 10.);
    let moved = egui::pos2(press.x + 20., rects[1].center().y);
    h.press(press);
    h.motion(moved);
    assert!(
        h.editor
            .document()
            .unwrap()
            .lane(&target)
            .unwrap()
            .clips
            .iter()
            .any(|clip| clip.id == before.id)
    );
    h.release(egui::pos2(moved.x, 780.));
    assert!(h.gesture.is_none());
    assert_eq!(h.editor.selected_lane.as_deref(), Some(target.as_str()));
    assert_eq!(h.editor.selected_clip.as_deref(), Some(before.id.as_str()));
    assert!(
        h.editor
            .document()
            .unwrap()
            .lane(&target)
            .unwrap()
            .clips
            .iter()
            .any(|clip| clip.id == before.id)
    );
    for (id, source) in sources {
        assert!(Arc::ptr_eq(&source, &h.editor.assets[&id]));
    }
}

#[test]
fn escape_restores_pre_gesture_range_and_the_exact_overlapping_eq_effect() {
    let mut h = Harness::new();
    let clip = h.first();
    let range = Some(FrameRange {
        start_frame: 800,
        end_frame: 2400,
    });
    let first = section(
        "earlier",
        EqScope::Clip {
            clip_id: clip.id.clone(),
        },
        range,
    );
    h.editor
        .apply(Edit::SetEq {
            effect: first.clone(),
        })
        .unwrap();
    let mut later = first.clone();
    later.id = "later".into();
    later.bands[2].gain = 1.;
    h.editor.apply(Edit::SetEq { effect: later }).unwrap();
    arrangement_eq_effects::select(&mut h.editor, &first).unwrap();
    let original_range = h.editor.selection;
    let original = h.editor.document().unwrap().clone();
    let rect = h.clips()[0];
    let press = egui::pos2(rect.left() + rect.width() * 0.55, rect.center().y);
    let moved = press + egui::vec2(25., 0.);
    h.press(press);
    h.motion(moved);
    assert_ne!(h.editor.selection, original_range);
    assert!(
        h.editor.draft.is_none(),
        "Selection does not create a metadata draft"
    );
    h.escape();
    assert!(h.gesture.is_none());
    assert_eq!(h.editor.selection, original_range);
    assert_eq!(h.editor.eq_id.as_deref(), Some("earlier"));
    assert_eq!(h.editor.eq_bands[2].gain, -2.);
    assert_eq!(h.editor.eq_effect().unwrap().range, range);
    h.release(moved);
    assert_eq!(h.editor.document().unwrap(), &original);
}

#[test]
fn escape_from_a_cross_lane_move_restores_metadata_and_previous_clip_selection() {
    let mut h = Harness::new();
    let original = h.editor.document().unwrap().clone();
    let selected = (
        h.editor.selected_lane.clone(),
        h.editor.selected_clip.clone(),
        h.editor.selection,
    );
    let rects = h.clips();
    let press = egui::pos2(rects[0].left() + 30., rects[0].top() + 10.);
    let moved = egui::pos2(press.x + 20., rects[1].center().y);
    h.press(press);
    h.motion(moved);
    assert!(h.editor.draft.is_some());
    h.escape();
    h.release(moved);
    assert_eq!(h.editor.document().unwrap(), &original);
    assert_eq!(
        (
            h.editor.selected_lane.clone(),
            h.editor.selected_clip.clone(),
            h.editor.selection
        ),
        selected
    );
}

#[test]
fn lane_eq_and_applied_lane_effect_clicks_keep_clip_selection_in_that_lane() {
    let mut h = Harness::new();
    let document = h.editor.document().unwrap().clone();
    let first = document.lanes[0].clone();
    let second = document.lanes[1].clone();
    h.click("EQ", 0);
    assert_eq!(h.editor.selected_lane.as_deref(), Some(first.id.as_str()));
    assert_eq!(
        h.editor.selected_clip.as_deref(),
        Some(first.clips[0].id.as_str())
    );
    assert_eq!(
        h.editor.eq_target_spec().unwrap().0,
        EqScope::Lane { lane_id: first.id }
    );
    let selected_range = Some(FrameRange {
        start_frame: 2800,
        end_frame: 4400,
    });
    h.editor.selection = selected_range;
    h.click("EQ", 0);
    assert_eq!(
        h.editor.selection, selected_range,
        "Opening EQ on the same lane must keep the selected section"
    );
    let effect = section(
        "lane_effect",
        EqScope::Lane {
            lane_id: second.id.clone(),
        },
        None,
    );
    h.editor.apply(Edit::SetEq { effect }).unwrap();
    h.effects = true;
    h.click("Applied EQ · 1 effects", 0);
    h.click("Edit", 0);
    assert_eq!(h.editor.selected_lane.as_deref(), Some(second.id.as_str()));
    assert_eq!(
        h.editor.selected_clip.as_deref(),
        Some(second.clips[0].id.as_str())
    );
    assert_eq!(h.editor.eq_id.as_deref(), Some("lane_effect"));
    assert_eq!(
        h.editor.eq_effect().unwrap().scope,
        EqScope::Lane { lane_id: second.id }
    );
}

#[test]
fn applied_eq_on_an_empty_lane_clears_the_previous_clip_selection() {
    let mut h = Harness::new();
    h.editor
        .apply(Edit::AddLane {
            lane: Lane::new("empty", "Empty lane"),
        })
        .unwrap();
    h.editor
        .apply(Edit::SetEq {
            effect: section(
                "empty_eq",
                EqScope::Lane {
                    lane_id: "empty".into(),
                },
                None,
            ),
        })
        .unwrap();
    h.effects = true;
    h.click("Applied EQ · 1 effects", 0);
    h.click("Edit", 0);
    assert_eq!(h.editor.selected_lane.as_deref(), Some("empty"));
    assert!(h.editor.selected_clip.is_none());
    assert_eq!(
        h.editor.eq_effect().unwrap().scope,
        EqScope::Lane {
            lane_id: "empty".into()
        }
    );
}

#[test]
fn applied_clip_effect_click_restores_its_source_scope_and_exact_effect_id() {
    let mut h = Harness::new();
    let first = h.first();
    let range = Some(FrameRange {
        start_frame: 800,
        end_frame: 2400,
    });
    let early = section(
        "early",
        EqScope::Clip {
            clip_id: first.id.clone(),
        },
        range,
    );
    h.editor
        .apply(Edit::SetEq {
            effect: early.clone(),
        })
        .unwrap();
    let mut late = early.clone();
    late.id = "late".into();
    late.bands[2].gain = 1.;
    h.editor.apply(Edit::SetEq { effect: late }).unwrap();
    h.effects = true;
    h.click("Applied EQ · 2 effects", 0);
    h.click("Edit", 0);
    let selected = h.editor.eq_effect().unwrap();
    assert_eq!(selected.id, early.id);
    assert_eq!(selected.scope, early.scope);
    assert_eq!(selected.range, early.range);
    assert_eq!(h.editor.eq_target, EqTarget::ClipRange);
    assert_eq!(
        h.editor.selection,
        Some(FrameRange {
            start_frame: first.start_frame + 800,
            end_frame: first.start_frame + 2400
        })
    );
}
