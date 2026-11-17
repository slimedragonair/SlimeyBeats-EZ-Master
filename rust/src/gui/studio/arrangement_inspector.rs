//! Selected-track and clip fields, including an exact alternative to canvas dragging.
use super::arrangement_state::ArrangementEditor;
use crate::{
    gui::{crystal_button, crystal_slider::CrystalSlider, theme},
    studio::arrangement::Edit,
};
use anyhow::Result;
use eframe::egui;

pub(super) fn show(editor: &mut ArrangementEditor, ui: &mut egui::Ui, playhead: f64) -> Result<()> {
    let Some(document) = editor.document() else {
        ui.label("Add tracks to start a multitrack session.");
        return Ok(());
    };
    let rate = document.sample_rate;
    let stereo = document.channels == 2;
    let lane_count = document.lanes.len();
    let lane = editor
        .selected_lane
        .as_deref()
        .and_then(|id| document.lane(id).ok())
        .cloned();
    let clip = editor
        .selected_clip
        .as_deref()
        .and_then(|id| document.clip(id).ok())
        .cloned();
    if let Some(lane) = lane {
        theme::eyebrow(ui, "SELECTED TRACK");
        if editor
            .lane_name_draft
            .as_ref()
            .is_none_or(|(id, _)| id != &lane.id)
        {
            editor.lane_name_draft = Some((lane.id.clone(), lane.name.clone()));
        }
        let name = &mut editor
            .lane_name_draft
            .as_mut()
            .expect("name draft initialized")
            .1;
        let renamed = ui
            .push_id(("arrangement-track-name", &lane.id), |ui| {
                ui.add(egui::TextEdit::singleline(name).desired_width(ui.available_width()))
                    .on_hover_text("Rename this track. The source file stays unchanged.")
            })
            .inner;
        if renamed.lost_focus() {
            editor.commit_lane_name()?;
        }
        let (mut mute, mut solo) = (lane.mute, lane.solo);
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            changed |= ui
                .toggle_value(&mut mute, "Mute")
                .on_hover_text("Silence this track, including when it is soloed.")
                .changed();
            changed |= ui
                .toggle_value(&mut solo, "Solo")
                .on_hover_text("Listen to soloed tracks. Muted tracks remain silent.")
                .changed();
        });
        if changed {
            editor.apply(Edit::SetLane {
                lane_id: lane.id.clone(),
                name: None,
                gain_db: None,
                pan: None,
                mute: Some(mute),
                solo: Some(solo),
            })?;
        }
        let mut gain = lane.gain_db;
        ui.label("Track level")
            .on_hover_text("Changes this track's level live; it does not alter its source audio.");
        let response = ui.add(
            CrystalSlider::new(&mut gain, -60. ..=12.)
                .suffix(" dB")
                .fixed_decimals(1),
        );
        if response.changed() {
            editor.preview_edit(Edit::SetLane {
                lane_id: lane.id.clone(),
                name: None,
                gain_db: Some(gain),
                pan: None,
                mute: None,
                solo: None,
            })?;
        }
        if response.drag_stopped() || (response.changed() && !response.dragged()) {
            editor.commit_draft()?;
        }
        let index = editor
            .document()
            .and_then(|document| document.lanes.iter().position(|other| other.id == lane.id))
            .unwrap_or(0);
        ui.horizontal_wrapped(|ui| -> Result<()> {
            for (label, target, enabled) in [
                ("Move track up", index.saturating_sub(1), index > 0),
                ("Move track down", index + 1, index + 1 < lane_count),
            ] {
                if ui
                    .add_enabled(enabled, crystal_button::secondary(label))
                    .clicked()
                {
                    editor.apply(Edit::ReorderLane {
                        lane_id: lane.id.clone(),
                        index: target,
                    })?;
                }
            }
            Ok(())
        })
        .inner?;
        let mut pan = lane.pan;
        ui.label("Balance")
            .on_hover_text("Move this track toward the left or right speaker.");
        let response = ui
            .add_enabled(
                stereo,
                CrystalSlider::new(&mut pan, -1. ..=1.).fixed_decimals(2),
            )
            .on_disabled_hover_text("Balance is available in stereo projects.");
        if response.changed() {
            editor.preview_edit(Edit::SetLane {
                lane_id: lane.id.clone(),
                name: None,
                gain_db: None,
                pan: Some(pan),
                mute: None,
                solo: None,
            })?;
        }
        if response.drag_stopped() || (response.changed() && !response.dragged()) {
            editor.commit_draft()?;
        }
    }
    ui.separator();
    super::arrangement_clip_controls::show(editor, ui, playhead, rate, clip)?;
    Ok(())
}
