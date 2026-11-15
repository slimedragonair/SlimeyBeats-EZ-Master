//! Selected-stem inspector: live mix controls and explicit, undoable time edits.
use super::{
    mix_controls,
    state::{Studio, Target},
};
use crate::{
    gui::{control_layout::section_header, crystal_button, theme},
    studio::region_edit::RegionOperation,
};
use eframe::egui;

pub(super) fn show(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context, advanced: bool) {
    let Some(bank) = studio.bank.clone() else {
        return;
    };
    studio.editor.sync(&bank);
    theme::eyebrow(ui, "SELECTED STEM");
    let previous = studio.editor.selected;
    let label = studio
        .editor
        .selected
        .map(|kind| bank.display_name(kind).into_owned())
        .unwrap_or_default();
    egui::ComboBox::from_id_salt("timeline-selected-part")
        .selected_text(label.as_str())
        .width((ui.available_width() - 12.).max(1.))
        .truncate()
        .show_ui(ui, |ui| {
            for stem in &bank.tracks {
                ui.selectable_value(
                    &mut studio.editor.selected,
                    Some(stem.kind),
                    bank.display_name(stem.kind),
                );
            }
        })
        .response
        .on_hover_text(
            "Choose the stem affected by time edits. Other parts and their timing stay unchanged.",
        );
    if studio.editor.selected != previous
        && let Some(kind) = studio.editor.selected
    {
        studio.target = Target::Stem(kind);
    }
    let available = !studio.busy();
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut studio.editor.inspector_edits, false, "Live mix");
        if ui
            .selectable_value(&mut studio.editor.inspector_edits, true, "Time edits")
            .clicked()
        {
            studio.editor.reveal_edits = true;
        }
    });
    if !studio.editor.inspector_edits {
        let mut changed = false;
        if let Some(kind) = studio.editor.selected
            && let Some(track) = studio
                .mix
                .tracks
                .iter_mut()
                .find(|track| track.kind == kind)
        {
            ui.add_enabled_ui(available, |ui| {
            let name = bank.display_name(kind);
            ui.label("Live level").on_hover_text("Changes how loud this part is in the mix immediately. It does not alter the waveform samples.");
            changed |= mix_controls::gain(track, ui, name.as_ref(), 360.);
            if advanced && bank.tracks[0].audio.channels == 2 { changed |= mix_controls::balance(track, ui, name.as_ref()); }
        });
        }
        if changed {
            studio.monitor_mix_changes();
        }
        ui.small("Drag a waveform to select time, or choose Time edits to enter an exact range.");
        return;
    }
    ui.separator();
    ui.add(egui::Label::new(format!("Edit: {label}")).truncate())
        .on_hover_text(format!(
            "Time edits affect {label} only. Other parts keep their timing."
        ));
    let audio = &bank.tracks[0].audio;
    let duration = audio.duration();
    let mut selection_changed = false;
    egui::Grid::new("editor-selection-fields")
        .num_columns(2)
        .show(ui, |ui| {
            for (label, value) in [
                ("Start", &mut studio.editor.draft_start),
                ("End", &mut studio.editor.draft_end),
            ] {
                let label = ui.label(label);
                selection_changed |= ui
                    .add(
                        egui::DragValue::new(value)
                            .range(0. ..=duration)
                            .clamp_existing_to_range(false)
                            .speed(0.01)
                            .suffix(" s")
                            .update_while_editing(false)
                            .max_decimals(9),
                    )
                    .labelled_by(label.id)
                    .changed();
                ui.end_row();
            }
        });
    if selection_changed {
        let (start, end) = (studio.editor.draft_start, studio.editor.draft_end);
        if start < end {
            studio
                .editor
                .select_time(start, end, audio.sample_rate, audio.frames());
        } else {
            studio.editor.selection = None;
        }
        if studio.editor.selection.is_none() {
            studio.editor.looping = false;
        }
        studio.update_editor_loop();
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().button_padding.x = 4.;
        ui.spacing_mut().item_spacing.x = 4.;
        if ui.small_button("Select all time").clicked() {
            studio
                .editor
                .select_time(0., duration, audio.sample_rate, audio.frames());
            studio.update_editor_loop();
        }
        if ui
            .add_enabled(
                studio.editor.selection.is_some(),
                egui::Button::new("Clear selection").small(),
            )
            .clicked()
        {
            studio.editor.selection = None;
            studio.editor.draft_start = 0.;
            studio.editor.draft_end = 0.;
            studio.editor.looping = false;
            studio.update_editor_loop();
        }
    });
    if ui.add_enabled(studio.editor.can_loop(), egui::Checkbox::new(&mut studio.editor.looping, "Loop selection")).on_hover_text("Repeat this time range across the whole mix while listening. This does not repeat audio in exports.").on_disabled_hover_text("Select at least 50 ms for loop audition, avoiding repeated DSP priming on tiny selections. Sample edits can still use a single frame.").changed() {
        studio.update_editor_loop();
    }
    let ready = available && !studio.mix_dirty && studio.editor.selection.is_some();
    if studio.mix_dirty {
        ui.small("Update mix preview to commit live controls before a time edit.");
    } else if studio.editor.selection.is_none() {
        ui.small("Select a nonempty time range to enable edits.");
    }
    let edit_actions = ui.add_enabled_ui(ready, |ui| {
        let actions = ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().button_padding.x = 4.;
            ui.spacing_mut().item_spacing.x = 4.;
            for (label, help, operation) in [
                ("Silence", "Replace the selected samples with silence; keep the gap and all stem timing.", RegionOperation::Silence),
                ("Fade in", "Raise this range gradually from silence to its current level.", RegionOperation::FadeIn),
                ("Fade out", "Lower this range gradually from its current level to silence.", RegionOperation::FadeOut),
            ] {
                if ui.add(crystal_button::secondary(label)).on_hover_text(help).clicked() { studio.edit_selected_region(operation, ctx); }
            }
        });
        section_header(ui, "More time edits").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let label = ui.label("Region gain");
                ui.add(egui::DragValue::new(&mut studio.editor.gain_db).range(-60. ..=12.).speed(0.1).suffix(" dB")).labelled_by(label.id);
                if ui.add(crystal_button::secondary("Apply gain")).on_hover_text("Change selected samples only. Positive gain can increase peaks; the mix's peak protection still applies.").clicked() { studio.edit_selected_region(RegionOperation::Gain { db: studio.editor.gain_db }, ctx); }
            });
            if ui.add(crystal_button::secondary("Reverse selection")).on_hover_text("Reverse frames inside the selected stem range, keeping left/right channels together and preserving its duration.").clicked() { studio.edit_selected_region(RegionOperation::Reverse, ctx); }
        });
        actions.response
    });
    if std::mem::take(&mut studio.editor.reveal_edits) {
        edit_actions.inner.scroll_to_me(Some(egui::Align::Max));
    }
    ui.small("Edits prepare a mix preview and can be undone. Export saves it separately. Original files stay unchanged.");
}
