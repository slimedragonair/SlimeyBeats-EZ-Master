//! Nondestructive clip fields and actions; source bounds stay separate from song time.
use super::{
    arrangement_fields::{frame, seconds},
    arrangement_state::ArrangementEditor,
};
use crate::{
    gui::{crystal_button, crystal_slider::CrystalSlider, theme},
    studio::arrangement::{Clip, Edit},
};
use anyhow::{Context, Result, ensure};
use eframe::egui;

pub(super) fn show(
    editor: &mut ArrangementEditor,
    ui: &mut egui::Ui,
    playhead: f64,
    rate: u32,
    clip: Option<Clip>,
) -> Result<()> {
    let Some(clip) = clip else {
        ui.label("Select a clip to move, trim, split or delete it.");
        return Ok(());
    };
    theme::eyebrow(ui, "SELECTED CLIP");
    let source_frames = editor
        .assets
        .get(&clip.source_id)
        .context("Clip source is unavailable")?
        .frames() as u64;
    let source_name = editor
        .asset_names
        .get(&clip.source_id)
        .cloned()
        .unwrap_or_else(|| clip.source_id.clone());
    ui.add(egui::Label::new(source_name).truncate())
        .on_hover_text("Original source; editing this clip never overwrites it.");
    if editor
        .clip_name_draft
        .as_ref()
        .is_none_or(|(id, _)| id != &clip.id)
    {
        editor.clip_name_draft = Some((clip.id.clone(), clip.name.clone().unwrap_or_default()));
    }
    let name = &mut editor
        .clip_name_draft
        .as_mut()
        .expect("name draft initialized")
        .1;
    let response = ui
        .push_id(("arrangement-clip-name", &clip.id), |ui| {
            ui.add(
                egui::TextEdit::singleline(name)
                    .hint_text("Clip name (optional)")
                    .desired_width(ui.available_width()),
            )
            .on_hover_text("Give this clip a name. Leave blank to use its source name.")
        })
        .inner;
    if response.lost_focus() {
        editor.commit_clip_name()?;
    }
    let mut position = clip.start_frame as f64 / rate as f64;
    let mut source_in = clip.source_start_frame as f64 / rate as f64;
    let mut source_out = clip.source_end_frame()? as f64 / rate as f64;
    let mut changed = false;
    egui::Grid::new(("arrangement-clip-bounds", &clip.id))
        .num_columns(2)
        .show(ui, |ui| {
            changed |= seconds(
                ui,
                "position",
                "Position",
                &mut position,
                "Where the clip starts in the song. Moving it keeps its source slice.",
            );
            changed |= seconds(
                ui,
                "source-in",
                "Source in",
                &mut source_in,
                "First source frame to keep; the song position remains unchanged.",
            );
            changed |= seconds(
                ui,
                "source-out",
                "Source out",
                &mut source_out,
                "Exclusive source end. No samples are deleted from the original file.",
            );
        });
    if changed {
        let start = frame(position, rate)?;
        let first = frame(source_in, rate)?;
        let end = frame(source_out, rate)?;
        ensure!(
            end <= source_frames,
            "Keep source bounds inside the original audio"
        );
        let length = end
            .checked_sub(first)
            .context("Source bounds are reversed")?;
        ensure!(length > 0, "Select at least one complete source frame");
        editor.apply(Edit::TrimClip {
            clip_id: clip.id.clone(),
            start_frame: start,
            source_start_frame: first,
            end_frame: start
                .checked_add(length)
                .context("Clip position overflow")?,
        })?;
        editor.select_clip(&clip.id)?;
    }
    ui.small(format!(
        "Ends at {:.3} s · {:.3} s long",
        clip.end_frame as f64 / rate as f64,
        clip.duration_frames() as f64 / rate as f64
    ));
    let mut gain = clip.gain_db;
    ui.label("Clip level")
        .on_hover_text("Adjust this clip before its track fader and whole-song processing.");
    let response = ui.add(
        CrystalSlider::new(&mut gain, -60. ..=12.)
            .suffix(" dB")
            .fixed_decimals(1),
    );
    if response.changed() {
        editor.preview_edit(Edit::SetClipGain {
            clip_id: clip.id.clone(),
            gain_db: gain,
        })?;
    }
    if response.drag_stopped() || (response.changed() && !response.dragged()) {
        editor.commit_draft()?;
    }
    let mut fade_in = clip.fade_in_frames as f64 / rate as f64;
    let mut fade_out = clip.fade_out_frames as f64 / rate as f64;
    let mut fades_changed = false;
    egui::Grid::new(("arrangement-clip-fades", &clip.id))
        .num_columns(2)
        .show(ui, |ui| {
            fades_changed |= seconds(
                ui,
                "fade-in",
                "Fade in",
                &mut fade_in,
                "Smoothly fade up from the start of this clip.",
            );
            fades_changed |= seconds(
                ui,
                "fade-out",
                "Fade out",
                &mut fade_out,
                "Smoothly fade down toward this clip's end.",
            );
        });
    if fades_changed {
        editor.apply(Edit::SetClipFades {
            clip_id: clip.id.clone(),
            fade_in_frames: frame(fade_in, rate)?,
            fade_out_frames: frame(fade_out, rate)?,
        })?;
    }
    let lane_id = editor
        .selected_lane
        .clone()
        .context("Select a track first")?;
    let split = (playhead.max(0.) * rate as f64).round() as u64;
    ui.horizontal_wrapped(|ui| -> Result<()> {
        if ui
            .add_enabled(
                split > clip.start_frame && split < clip.end_frame,
                crystal_button::secondary("Split at playhead"),
            )
            .on_hover_text(
                "Create two source-backed pieces; their section EQ follows the original samples.",
            )
            .clicked()
        {
            let id = editor.fresh_id("clip");
            editor.apply(Edit::SplitClip {
                clip_id: clip.id.clone(),
                at_frame: split,
                new_clip_id: id.clone(),
            })?;
            editor.select_clip(&id)?;
        }
        if ui
            .add(crystal_button::secondary("Duplicate after"))
            .on_hover_text("Place a copy immediately after this clip on the same track.")
            .clicked()
        {
            let id = editor.fresh_id("clip");
            editor.apply(Edit::DuplicateClip {
                clip_id: clip.id.clone(),
                new_clip_id: id.clone(),
                lane_id: lane_id.clone(),
                start_frame: clip.end_frame,
            })?;
            editor.select_clip(&id)?;
        }
        if ui
            .add(crystal_button::secondary("Delete clip"))
            .on_hover_text(
                "Remove the clip from the song. Undo restores it; its source stays available.",
            )
            .clicked()
        {
            editor.apply(Edit::DeleteClip {
                clip_id: clip.id.clone(),
            })?;
        }
        Ok(())
    })
    .inner?;
    Ok(())
}
