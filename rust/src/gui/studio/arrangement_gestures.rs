//! A pointer gesture previews metadata and commits one undo step on release.
use super::arrangement_state::{ArrangementEditor, EqTarget};
use crate::studio::arrangement::{Clip, Edit, FrameRange};
use anyhow::Result;
use eframe::egui::{self, Pos2, Rect};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Move,
    TrimStart,
    TrimEnd,
    Select,
}
pub(super) struct Gesture {
    pub kind: Kind,
    pub clip: Clip,
    pub lane: String,
    pub pointer_start: Pos2,
    pub anchor_frame: u64,
    pub plot_width: f32,
    pub span_seconds: f64,
    pub rate: u32,
    pub previous_selection: SelectionSnapshot,
}

pub(super) struct SelectionSnapshot {
    lane: Option<String>,
    clip: Option<String>,
    range: Option<FrameRange>,
    target: EqTarget,
    effect: Option<String>,
}
impl SelectionSnapshot {
    pub fn capture(editor: &ArrangementEditor) -> Self {
        Self {
            lane: editor.selected_lane.clone(),
            clip: editor.selected_clip.clone(),
            range: editor.selection,
            target: editor.eq_target,
            effect: editor.eq_id.clone(),
        }
    }
    fn restore(self, editor: &mut ArrangementEditor) {
        editor.selected_lane = self.lane;
        editor.selected_clip = self.clip;
        editor.selection = self.range;
        editor.eq_target = self.target;
        editor.eq_id = self.effect;
        editor.refresh_eq();
    }
}

pub(super) fn update(
    gesture: &mut Option<Gesture>,
    editor: &mut ArrangementEditor,
    rows: &[(String, Rect)],
    ui: &egui::Ui,
) -> Result<()> {
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        let cancelled = gesture.take();
        editor.cancel_draft();
        if let Some(cancelled) = cancelled {
            cancelled.previous_selection.restore(editor);
        }
        return Ok(());
    }
    let Some(current) = gesture.as_mut() else {
        return Ok(());
    };
    if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
        let delta = ((pointer.x - current.pointer_start.x) as f64
            / current.plot_width.max(1.) as f64
            * current.span_seconds
            * current.rate as f64)
            .round() as i128;
        match current.kind {
            Kind::Move => {
                let maximum =
                    crate::studio::arrangement::MAX_FRAME - current.clip.duration_frames();
                let start =
                    (current.clip.start_frame as i128 + delta).clamp(0, maximum as i128) as u64;
                let lane = rows
                    .iter()
                    .find(|(_, rect)| pointer.y >= rect.top() && pointer.y < rect.bottom())
                    .map(|(id, _)| id.clone())
                    .unwrap_or_else(|| current.lane.clone());
                editor.preview_edit(Edit::MoveClip {
                    clip_id: current.clip.id.clone(),
                    lane_id: lane.clone(),
                    start_frame: start,
                })?;
                current.lane = lane;
            }
            Kind::TrimStart => {
                let minimum = current
                    .clip
                    .start_frame
                    .saturating_sub(current.clip.source_start_frame);
                let start = (current.clip.start_frame as i128 + delta)
                    .clamp(minimum as i128, current.clip.end_frame as i128 - 1)
                    as u64;
                let source_start = (current.clip.source_start_frame as i128 + start as i128
                    - current.clip.start_frame as i128) as u64;
                editor.preview_edit(Edit::TrimClip {
                    clip_id: current.clip.id.clone(),
                    start_frame: start,
                    source_start_frame: source_start,
                    end_frame: current.clip.end_frame,
                })?;
            }
            Kind::TrimEnd => {
                if let Some(source) = editor.assets.get(&current.clip.source_id) {
                    let maximum = current.clip.start_frame + source.frames() as u64
                        - current.clip.source_start_frame;
                    let end = (current.clip.end_frame as i128 + delta)
                        .clamp(current.clip.start_frame as i128 + 1, maximum as i128)
                        as u64;
                    editor.preview_edit(Edit::TrimClip {
                        clip_id: current.clip.id.clone(),
                        start_frame: current.clip.start_frame,
                        source_start_frame: current.clip.source_start_frame,
                        end_frame: end,
                    })?;
                }
            }
            Kind::Select => {
                let end = (current.anchor_frame as i128 + delta).clamp(
                    current.clip.start_frame as i128,
                    current.clip.end_frame as i128,
                ) as u64;
                let first = current.anchor_frame.min(end);
                let last = current
                    .anchor_frame
                    .max(end)
                    .max(first + 1)
                    .min(current.clip.end_frame);
                editor.selection = Some(FrameRange {
                    start_frame: first.min(last - 1),
                    end_frame: last,
                });
                editor.eq_proposal = None;
            }
        }
    }
    if ui.input(|input| input.pointer.any_released() || !input.pointer.primary_down()) {
        let selected = current.clip.id.clone();
        let geometric = current.kind != Kind::Select;
        editor.commit_draft()?;
        *gesture = None;
        if geometric {
            editor.select_clip(&selected)?;
        } else {
            editor.refresh_eq();
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "arrangement_gesture_tests.rs"]
mod tests;
