//! Clearly scoped live EQ and a separate proposal/accept action.
use super::{
    arrangement_fields::{frame, seconds},
    arrangement_state::{ArrangementEditor, EqTarget},
};
use crate::{
    gui::{crystal_button, crystal_slider::CrystalSlider, theme},
    studio::arrangement::{Edit, FrameRange, ScopedEq},
};
use anyhow::{Result, ensure};
use eframe::egui;

pub(super) enum Action {
    Suggest(ScopedEq),
}

pub(super) fn show(
    editor: &mut ArrangementEditor,
    ui: &mut egui::Ui,
    advanced: bool,
) -> Result<Option<Action>> {
    super::arrangement_eq_effects::show(editor, ui)?;
    let Some(document) = editor.document() else {
        return Ok(None);
    };
    let rate = document.sample_rate;
    theme::eyebrow(ui, "EQ TARGET");
    let mut base = match editor.eq_target {
        EqTarget::Mix | EqTarget::MixRange => 0,
        EqTarget::Lane | EqTarget::LaneRange => 1,
        _ => 2,
    };
    let mut ranged = matches!(
        editor.eq_target,
        EqTarget::MixRange | EqTarget::LaneRange | EqTarget::ClipRange
    );
    let previous = (base, ranged);
    egui::ComboBox::from_id_salt("arrangement-eq-target")
        .selected_text(["Whole song", "Selected track", "Selected clip"][base])
        .width((ui.available_width() - 10.).max(80.))
        .show_ui(ui, |ui| {
            for (index, label) in ["Whole song", "Selected track", "Selected clip"]
                .into_iter()
                .enumerate()
            {
                ui.selectable_value(&mut base, index, label);
            }
        });
    ui.checkbox(&mut ranged, "Only selected time range")
        .on_hover_text("Use the highlighted timeline interval. Clip EQ stays attached to the same source audio when you move the clip.");
    editor.eq_target = match (base, ranged) {
        (0, false) => EqTarget::Mix,
        (0, true) => EqTarget::MixRange,
        (1, false) => EqTarget::Lane,
        (1, true) => EqTarget::LaneRange,
        (_, false) => EqTarget::Clip,
        _ => EqTarget::ClipRange,
    };
    if previous != (base, ranged) {
        editor.refresh_eq();
    }
    if ranged {
        let mut start = editor
            .selection
            .map_or(0., |range| range.start_frame as f64 / rate as f64);
        let mut end = editor
            .selection
            .map_or(0., |range| range.end_frame as f64 / rate as f64);
        let mut changed = false;
        egui::Grid::new("arrangement-eq-range")
            .num_columns(2)
            .show(ui, |ui| {
                changed |= seconds(
                    ui,
                    "start",
                    "Start",
                    &mut start,
                    "Start of the selected section in the song.",
                );
                changed |= seconds(
                    ui,
                    "end",
                    "End",
                    &mut end,
                    "Exclusive end of the section. Use at least 20 ms for smooth EQ transitions.",
                );
            });
        if changed {
            editor.selection = Some(FrameRange {
                start_frame: frame(start, rate)?,
                end_frame: frame(end, rate)?,
            });
            editor.refresh_eq();
        }
    }
    ui.label(egui::RichText::new(target_label(editor)).color(theme::BONE));
    if let Err(error) = editor.eq_target_spec() {
        ui.colored_label(theme::BONE, error.to_string());
        return Ok(None);
    }
    super::arrangement_eq_curve::show(editor, ui, rate);
    let mut action = None;
    ui.horizontal_wrapped(|ui| -> Result<()> {
        if ui.add(crystal_button::secondary("Suggest EQ")).on_hover_text("Measure only this target, leaving its current EQ unchanged until Apply curve.").clicked() {
            action = Some(Action::Suggest(editor.eq_effect()?));
        }
        if ui.add_enabled(editor.eq_proposal.is_some(), crystal_button::primary("Apply curve")).clicked() {
            let proposal = editor.eq_proposal.take().expect("enabled proposal");
            editor.eq_bands = proposal.bands; editor.eq_bypass = false;
            let effect = editor.eq_effect()?;
            editor.apply(Edit::SetEq { effect })?;
            editor.eq_notice = Some("Suggested curve applied to this target. Play B to compare; export remains separate.".into());
        }
        Ok(())
    }).inner?;
    if let Some(note) = &editor.eq_notice {
        ui.small(note);
    }
    if let Some(proposal) = &editor.eq_proposal {
        ui.small(
            if proposal
                .high_frequency
                .as_ref()
                .is_some_and(|high| high.excess_candidate)
            {
                "Possible excess upper treble. The pale curve is only a proposal."
            } else {
                "Review the pale curve before applying it."
            },
        );
        egui::CollapsingHeader::new("Why this curve?").show(ui, |ui| {
            for note in &proposal.notes {
                ui.label(note);
            }
        });
    }
    let mut bypass = editor.eq_bypass;
    if ui.checkbox(&mut bypass, "Bypass this EQ").changed() {
        editor.eq_bypass = bypass;
        let effect = editor.eq_effect()?;
        editor.apply(Edit::SetEq { effect })?;
    }
    ui.separator();
    editor.eq_selected_band = editor.eq_selected_band.min(6);
    egui::ComboBox::from_id_salt("arrangement-eq-band")
        .selected_text(format!(
            "Band {} · {:.0} Hz",
            editor.eq_selected_band + 1,
            editor.eq_bands[editor.eq_selected_band].freq
        ))
        .show_ui(ui, |ui| {
            for index in 0..7 {
                ui.selectable_value(
                    &mut editor.eq_selected_band,
                    index,
                    format!("{} · {:.0} Hz", index + 1, editor.eq_bands[index].freq),
                );
            }
        });
    let mut changed = false;
    let mut commit = false;
    let band = &mut editor.eq_bands[editor.eq_selected_band];
    let response = ui.checkbox(&mut band.on, "Band enabled");
    changed |= response.changed();
    commit |= response.changed();
    ui.label("Gain").on_hover_text("Positive values lift this frequency area. Negative values reduce it. Changes audition live on the selected target.");
    let available = band.freq <= rate as f64 * 0.49;
    let response = ui.add_enabled(
        available,
        CrystalSlider::new(&mut band.gain, -18. ..=18.)
            .suffix(" dB")
            .fixed_decimals(2),
    );
    if !available {
        ui.small("This band is above the project bandwidth. Choose another band or change its frequency.");
    }
    changed |= response.changed();
    commit |= response.drag_stopped() || (response.changed() && !response.dragged());
    if advanced {
        ui.horizontal_wrapped(|ui| {
            ui.label("Frequency");
            let response = ui.add(
                egui::DragValue::new(&mut band.freq)
                    .speed(10.)
                    .suffix(" Hz")
                    .update_while_editing(false),
            );
            changed |= response.changed();
            commit |= response.changed();
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Width / Q").on_hover_text("A smaller Q affects a wider frequency area. A larger Q makes a narrower adjustment.");
            let response = ui.add(egui::DragValue::new(&mut band.q).speed(0.01).update_while_editing(false));
            changed |= response.changed(); commit |= response.changed();
        });
        let response = egui::ComboBox::from_id_salt("arrangement-eq-shape")
            .selected_text(&band.kind)
            .show_ui(ui, |ui| {
                let mut selected = false;
                for kind in ["bell", "low_shelf", "high_shelf", "low_cut", "high_cut"] {
                    selected |= ui
                        .selectable_value(&mut band.kind, kind.to_owned(), kind.replace('_', " "))
                        .changed();
                }
                selected
            })
            .inner
            .unwrap_or(false);
        changed |= response;
        commit |= response;
        ui.label("EQ output level");
        let response =
            ui.add(CrystalSlider::new(&mut editor.eq_output_db, -24. ..=12.).suffix(" dB"));
        changed |= response.changed();
        commit |= response.drag_stopped() || (response.changed() && !response.dragged());
    }
    if changed {
        ensure!(
            editor
                .eq_bands
                .iter()
                .all(|band| band.freq <= rate as f64 * 0.49
                    || !band.on
                    || (band.gain == 0. && !band.kind.ends_with("cut"))),
            "This EQ frequency is above the project's usable bandwidth"
        );
        editor.eq_proposal = None;
        let effect = editor.eq_effect()?;
        editor.preview_edit(Edit::SetEq { effect })?;
    }
    if commit {
        editor.commit_draft()?;
    }
    ui.small("Clip, track and whole-song effects combine. Use Bypass and Undo while comparing.");
    Ok(action)
}

/// Keep the active object and song-time selection visible without exposing internal IDs.
pub(super) fn target_label(editor: &ArrangementEditor) -> String {
    let Some(document) = editor.document() else {
        return "Add audio to choose an EQ target".into();
    };
    let (base, ranged) = match editor.eq_target {
        EqTarget::Mix => (0, false),
        EqTarget::MixRange => (0, true),
        EqTarget::Lane => (1, false),
        EqTarget::LaneRange => (1, true),
        EqTarget::Clip => (2, false),
        EqTarget::ClipRange => (2, true),
    };
    let mut label = match base {
        0 => if ranged { "Song section" } else { "Whole song" }.to_owned(),
        1 => editor
            .selected_lane
            .as_deref()
            .and_then(|id| document.lane(id).ok())
            .map(|lane| format!("Track: {}", lane.name))
            .unwrap_or_else(|| "Select a track".into()),
        _ => editor
            .selected_clip
            .as_deref()
            .and_then(|id| document.clip(id).ok())
            .map(|clip| {
                format!(
                    "Clip: {}",
                    clip.name
                        .as_ref()
                        .or_else(|| editor.asset_names.get(&clip.source_id))
                        .map_or("Audio clip", String::as_str)
                )
            })
            .unwrap_or_else(|| "Select a clip".into()),
    };
    if ranged && let Some(range) = editor.selection {
        label.push_str(&format!(
            " · {:.3}–{:.3} s in song",
            range.start_frame as f64 / document.sample_rate as f64,
            range.end_frame as f64 / document.sample_rate as f64
        ));
    }
    label
}
