//! Multitrack center and selected clip/EQ inspector, retaining the app's native theme.
use super::{arrangement_eq_controls, arrangement_timeline, state::Studio};
use crate::gui::{crystal_button, mastering_controls::format_label};
use eframe::egui;
use std::sync::atomic::Ordering;

pub(super) fn show(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
    if studio
        .bulk_import
        .as_ref()
        .is_some_and(super::stem_bulk_import::Review::is_arrangement)
    {
        super::stem_bulk_import::review(studio, ui, ctx, ui.available_width() < 700.);
        return false;
    }
    let ready = !studio.busy();
    ui.add_enabled_ui(ready, |ui| {
        super::arrangement_toolbar::show(studio, ui, ctx);
        ui.separator();
        let position = studio.player.seconds();
        let outcome = arrangement_timeline::show(
            &mut studio.arrangement.editor,
            &studio.arrangement.waveforms,
            &mut studio.arrangement.gesture,
            ui,
            position,
        );
        match outcome {
            Ok(Some(frame)) => {
                if let Some(document) = studio.arrangement.editor.document() {
                    studio
                        .player
                        .seek(frame as f64 / document.sample_rate as f64);
                }
            }
            Ok(None) => {}
            Err(error) => studio.error = Some(error.to_string()),
        }
    });
    sync(studio, ctx);
    std::mem::take(&mut studio.arrangement.promote)
}

pub(super) fn sidebar(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context, advanced: bool) {
    egui::Panel::bottom("arrangement-save-export")
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            ui.separator();
            let audio = studio
                .arrangement
                .editor
                .document()
                .is_some_and(|document| document.duration_frames() > 0);
            let enabled = !studio.busy();
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(enabled, crystal_button::secondary("Open project…"))
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Studio project", &["json"])
                        .pick_file()
                {
                    studio.open_arrangement(path, ctx);
                }
                if ui
                    .add_enabled(
                        enabled && studio.arrangement.editor.history.is_some(),
                        crystal_button::secondary("Save project…"),
                    )
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_directory(&studio.output_dir)
                        .pick_folder()
                {
                    studio.save_arrangement(path, ctx);
                }
            });
            if ui
                .add_enabled(
                    enabled && audio,
                    crystal_button::primary("Export timeline mix"),
                )
                .clicked()
            {
                studio.export_arrangement(ctx);
            }
            ui.horizontal_wrapped(|ui| {
                ui.label("Format");
                egui::ComboBox::from_id_salt("arrangement-format")
                    .selected_text(format_label(&studio.export_format))
                    .width(140.)
                    .show_ui(ui, |ui| {
                        for format in crate::config::FORMATS {
                            ui.selectable_value(
                                &mut studio.export_format,
                                (*format).into(),
                                format_label(format),
                            );
                        }
                    });
                if ui.small_button("Folder…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_directory(&studio.output_dir)
                        .pick_folder()
                {
                    studio.output_dir = path;
                }
            });
        });
    let enabled = !studio.busy();
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(
            &mut studio.arrangement.editor.eq_inspector,
            false,
            "Clip / track",
        );
        ui.selectable_value(&mut studio.arrangement.editor.eq_inspector, true, "EQ");
    });
    egui::ScrollArea::vertical()
        .id_salt("arrangement-inspector")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_max_width((ui.available_width() - 1. / ui.pixels_per_point()).max(1.));
            ui.add_enabled_ui(enabled, |ui| {
                if studio
                    .bulk_import
                    .as_ref()
                    .is_some_and(super::stem_bulk_import::Review::is_arrangement)
                {
                    super::stem_bulk_import::actions(studio, ui, ctx);
                } else if studio.arrangement.editor.eq_inspector {
                    match arrangement_eq_controls::show(
                        &mut studio.arrangement.editor,
                        ui,
                        advanced,
                    ) {
                        Ok(Some(arrangement_eq_controls::Action::Suggest(target))) => {
                            studio.suggest_arrangement_eq(target, ctx)
                        }
                        Ok(None) => {}
                        Err(error) => studio.error = Some(error.to_string()),
                    }
                } else if let Err(error) = super::arrangement_inspector::show(
                    &mut studio.arrangement.editor,
                    ui,
                    studio.player.seconds(),
                ) {
                    studio.error = Some(error.to_string());
                }
            });
        });
}

fn sync(studio: &mut Studio, ctx: &egui::Context) {
    if studio.player.has_live_arrangement() {
        studio.arrangement.position = studio.player.seconds();
        if studio.arrangement.sent_epoch != Some(studio.arrangement.editor.epoch)
            && let Some(document) = studio.arrangement.editor.document()
        {
            match studio.player.set_arrangement(
                document,
                &studio.arrangement.editor.assets,
                &studio.arrangement.options,
            ) {
                Ok(true) => {
                    studio.arrangement.sent_epoch = Some(studio.arrangement.editor.epoch);
                    studio.player.controls.after.store(true, Ordering::Relaxed);
                }
                Ok(false) => {}
                Err(error) => studio.error = Some(error.to_string()),
            }
        }
        if let Some(error) = studio.player.arrangement_error() {
            studio.error = Some(error);
        }
    } else if studio
        .arrangement
        .editor
        .document()
        .is_some_and(|document| document.duration_frames() > 0)
        && !studio.busy()
    {
        studio.prepare_arrangement_player(ctx);
    }
    if studio.player.controls.playing.load(Ordering::Relaxed)
        || studio.player.arrangement_preparing()
    {
        ctx.request_repaint_after(std::time::Duration::from_millis(25));
    }
}
