//! Keep import/history reachable while reserving useful height for clip lanes.
use super::state::Studio;
use crate::gui::crystal_button;
use eframe::egui;

pub(super) fn show(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) {
    let compact = ui.ctx().viewport_rect().height() < 800. || ui.available_width() < 800.;
    ui.horizontal_wrapped(|ui| {
        if ui
            .add(crystal_button::primary("Add tracks…"))
            .on_hover_text(
                "Load one or many source-backed clips. Different track lengths are supported.",
            )
            .clicked()
            && let Some(paths) = rfd::FileDialog::new()
                .add_filter("Audio", crate::audio::EXTENSIONS)
                .pick_files()
        {
            studio.prepare_arrangement_files(paths, ctx);
        }
        if ui
            .add(crystal_button::secondary("Add Suno / stem ZIP…"))
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Audio ZIP", &["zip"])
                .pick_file()
        {
            studio.prepare_arrangement_archive(path, ctx);
        }
        let undo = studio
            .arrangement
            .editor
            .history
            .as_ref()
            .is_some_and(|history| history.can_undo());
        let redo = studio
            .arrangement
            .editor
            .history
            .as_ref()
            .is_some_and(|history| history.can_redo());
        for (label, allowed, forward) in [("Undo", undo, false), ("Redo", redo, true)] {
            if ui
                .add_enabled(allowed, crystal_button::secondary(label))
                .clicked()
                && let Err(error) = studio.arrangement.editor.history_step(forward)
            {
                studio.error = Some(error.to_string());
            }
        }
        {
            ui.menu_button("Track actions", |ui| {
                if ui.add_enabled(studio.arrangement.editor.history.is_some(), egui::Button::new("Add empty track"))
                    .on_disabled_hover_text("Add audio first so the new track uses the project's sample rate and channels.").clicked() {
                    if let Err(error) = super::arrangement_track_actions::add_empty(&mut studio.arrangement.editor) {
                        studio.error = Some(error.to_string());
                    }
                    ui.close();
                }
                if studio.bank.is_some()
                    && ui
                        .button("Copy imported stems")
                        .on_hover_text("Keep the stems' current level, mute, solo and balance.")
                        .clicked()
                {
                    studio.copy_bank_to_arrangement(ctx);
                    ui.close();
                }
                if studio.source.is_some() && ui.button("Add loaded mix").clicked() {
                    studio.copy_source_to_arrangement(ctx);
                    ui.close();
                }
            });
        }
        let populated = studio
            .arrangement
            .editor
            .document()
            .is_some_and(|document| document.duration_frames() > 0);
        if compact {
            ui.add_enabled_ui(populated, |ui| {
                ui.menu_button("Send mix…", |ui| targets(studio, ui, ctx, true));
            });
        } else {
            ui.add_enabled_ui(populated, |ui| targets(studio, ui, ctx, false));
        }
    });
}
fn targets(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context, close_menu: bool) {
    for (label, promote) in [("Use for mastering", true), ("Use mix in tools", false)] {
        if ui.add(crystal_button::secondary(label)).on_hover_text("Prepare this timeline snapshot for the selected workflow. This does not export audio or alter sources.").clicked() {
            studio.render_arrangement(promote, ctx); if close_menu { ui.close(); }
        }
    }
}
