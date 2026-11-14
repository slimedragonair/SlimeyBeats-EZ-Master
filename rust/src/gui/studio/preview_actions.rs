//! Pinned preparation, apply and saving actions. Rendering never implies export.
use super::state::{Page, Studio, Target};
use crate::gui::{crystal_button, mastering_controls::format_label};
use eframe::egui;

pub(super) fn preview_unavailable(studio: &Studio) -> Option<&'static str> {
    if studio.busy() {
        Some("Wait for the current job or cancel it first.")
    } else if studio.mix_dirty {
        Some(
            "Live stem changes are audible. Update mix preview before saving or further processing.",
        )
    } else if studio.current.is_none() {
        Some("Load audio or import stems to prepare a preview.")
    } else {
        None
    }
}

pub(super) fn edit_unavailable(studio: &Studio) -> Option<String> {
    if studio.busy() {
        Some("Wait for the current job or cancel it first.".into())
    } else {
        studio.editing_target().err().map(|error| error.to_string())
    }
}

pub(super) fn dock(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) {
    egui::Panel::bottom("studio-export").frame(egui::Frame::NONE).show(ui, |ui| {
        ui.separator();
        if (studio.page == Page::Stems || studio.mix_dirty) && studio.bank.is_some() && studio.bulk_import.is_none() {
            let label = if studio.mix_dirty { "Update mix preview · pending" } else { "Update mix preview" };
            if ui.add_enabled(!studio.busy(), crystal_button::primary(label)).on_hover_text("Prepare and measure the current live fader, mute, solo and balance settings for saving or further processing. Listening updates live without this step. This does not export.").clicked() { studio.render_mix(ctx); }
        }
        let action = match studio.page {
            Page::Voice => Some(("Preview voice cleanup", Studio::clean_voice as fn(&mut Studio, &egui::Context))),
            Page::Repair => Some(("Preview spectral repair", Studio::repair_audio as fn(&mut Studio, &egui::Context))),
            Page::Plugins => studio.plugin_probe.as_ref().filter(|probe| probe.plugins.iter().any(|info| info.class.id == studio.plugin_class && info.error.is_none())).map(|_| ("Preview plugin effect", Studio::render_plugin as fn(&mut Studio, &egui::Context))),
            Page::Stems => None,
        };
        let edit_reason = edit_unavailable(studio);
        if let Some((label, prepare)) = action
            && ui.add_enabled(edit_reason.is_none(), crystal_button::primary(label)).on_hover_text("Prepare audio for A/B listening using the visible tool settings. This does not save a delivery file.").clicked() { prepare(studio, ctx); }
        if studio.page != Page::Stems && matches!(studio.target, Target::Stem(_)) {
            let ready = edit_reason.is_none() && studio.can_apply_preview_to_stem();
            if ui.add_enabled(ready, crystal_button::secondary("Apply preview to selected stem").wrap()).on_disabled_hover_text("Preview a direct edit from this stem first; update pending mix changes before applying it.").clicked() { studio.apply_preview_to_stem(ctx); }
            ui.small("Apply rebuilds the mix; Export saves prepared audio.");
        }
        let reason = preview_unavailable(studio);
        if ui.add_enabled(reason.is_none(), crystal_button::primary("Export current preview")).on_hover_text("Save the prepared Studio preview and a report. New tool settings need Preview first.").clicked() { studio.export_preview(ctx); }
        if let Some(reason) = reason { ui.small(reason); }
        else if action.is_some() && let Some(reason) = edit_reason { ui.small(reason); }
        ui.horizontal_wrapped(|ui| {
            let label = ui.label("Format");
            egui::ComboBox::from_id_salt("studio-format").selected_text(format_label(&studio.export_format)).width(148.).show_ui(ui, |ui| {
                for format in crate::config::FORMATS { ui.selectable_value(&mut studio.export_format, (*format).into(), format_label(format)); }
            }).response.labelled_by(label.id);
            if ui.add(crystal_button::secondary("Folder…")).on_hover_text("Choose the Studio export folder. This is remembered separately from mastering.").clicked()
                && let Some(path) = rfd::FileDialog::new().set_directory(&studio.output_dir).pick_folder() { studio.output_dir = path; }
        });
        ui.add(egui::Label::new(egui::RichText::new(studio.output_dir.to_string_lossy()).small()).truncate()).on_hover_text(studio.output_dir.to_string_lossy());
        if studio.bank.is_some()
            && ui.add_enabled(!studio.busy(), crystal_button::secondary("Export raw stems")).on_hover_text("Aligned 32-bit float WAVs from the current bank, including applied stem edits. Faders, mute and solo are ignored; existing folders are preserved.").clicked() { studio.export_stems(ctx); }
    });
}
