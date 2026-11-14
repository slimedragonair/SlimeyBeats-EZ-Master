use super::{
    state::{Page, Studio},
    transport,
};
use crate::gui::{crystal_button, theme};
use eframe::egui::{self, RichText};

/// Returns true when the user explicitly sends the prepared mix to mastering.
pub fn show(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context, advanced: bool) -> bool {
    if studio.spectrogram.is_none() {
        studio.reveal_spectrogram = false;
    }
    let mut promote = false;
    let compact = ui.available_height() < 800.;
    transport::dock(studio, ui, compact);
    if !compact {
        ui.horizontal_wrapped(|ui| {
            if studio.busy() {
                ui.spinner();
                ui.add(egui::ProgressBar::new(studio.progress).desired_width(140.));
                if ui.add(crystal_button::secondary("Cancel")).clicked() {
                    studio.cancel();
                }
            }
            ui.label(
                egui::RichText::new(&studio.status)
                    .color(theme::MUTED)
                    .size(13.),
            );
        });
    }
    let tabs = |studio: &mut Studio, ui: &mut egui::Ui| {
        for (page, name) in [
            (Page::Stems, if compact { "Stems" } else { "Stem Studio" }),
            (Page::Voice, if compact { "Voice" } else { "Voice cleanup" }),
            (
                Page::Repair,
                if compact { "Repair" } else { "Spectral repair" },
            ),
            (Page::Plugins, "Plugins"),
        ] {
            if ui
                .add(crystal_button::secondary(name).selected(studio.page == page))
                .clicked()
            {
                studio.page = page;
            }
        }
    };
    let history = |studio: &mut Studio, ui: &mut egui::Ui| -> bool {
        if ui
            .add_enabled(
                !studio.busy() && !studio.undo.is_empty(),
                crystal_button::secondary("Undo"),
            )
            .on_disabled_hover_text(if studio.busy() {
                "Wait for the current job or cancel it first."
            } else {
                "No earlier Studio preview to restore."
            })
            .clicked()
        {
            studio.history(false);
        }
        if ui
            .add_enabled(
                !studio.busy() && !studio.redo.is_empty(),
                crystal_button::secondary("Redo"),
            )
            .on_disabled_hover_text(if studio.busy() {
                "Wait for the current job or cancel it first."
            } else {
                "No undone Studio preview to restore."
            })
            .clicked()
        {
            studio.history(true);
        }
        ui.add_enabled(super::controls::preview_unavailable(studio).is_none(),crystal_button::secondary(if compact {"To master"}else{"Use for mastering"})).on_hover_text("Add a private working copy to the mastering queue. This prepares another stage; it does not export a final delivery file.").on_disabled_hover_text(super::controls::preview_unavailable(studio).unwrap_or_default()).clicked()
    };
    if compact {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 3.;
            ui.spacing_mut().button_padding.x = 5.;
            ui.horizontal_wrapped(|ui| {
                tabs(studio, ui);
                promote = history(studio, ui);
            });
        });
    } else {
        ui.horizontal_wrapped(|ui| tabs(studio, ui));
        ui.horizontal_wrapped(|ui| {
            promote = history(studio, ui);
        });
    }
    if let Some(error) = studio.error.clone() {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(62, 29, 44))
            .inner_margin(10)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("studio-notice")
                    .max_height(if compact { 75. } else { 140. })
                    .show(ui, |ui| {
                        ui.label(error);
                    });
                if ui.small_button("Dismiss").clicked() {
                    studio.error = None;
                }
            });
    }
    if !compact && let Some(analysis) = &studio.analysis {
        ui.label(format!(
            "Selected input · {:.1} LUFS · {:.2} dBTP · {} Hz",
            analysis.metrics.integrated_lufs, analysis.metrics.true_peak_dbtp, analysis.sample_rate
        ));
    }
    let workspace_height = ui.available_height().max(0.);
    egui::ScrollArea::vertical().id_salt(("studio-workspace", studio.page)).max_height(workspace_height).show(ui,|ui| {
        match studio.page {
            Page::Stems=>{
                if studio.bulk_import.is_some() {
                    ui.label(theme::display("Review your stem set.",if compact {20.} else {24.}));
                    if compact {
                        ui.small("Choose parts and format, then import from the right panel.");
                    } else {
                        ui.label("Review the parts, then import from the right panel. Leave the full mix out when importing its individual stems.");
                    }
                    let available = !studio.busy();
                    ui.add_enabled_ui(available, |ui| super::stem_bulk_import::review(studio,ui,ctx,compact));
                } else if studio.bank.is_some() {
                    if !compact { ui.label(theme::display("Mix your stems.",24.)); }
                    super::mix_controls::show(studio, ui, advanced, compact);
                } else {
                ui.label(theme::display("Separate. Rebalance. Refine.",24.));
                ui.label("Import existing stems with no model download. Use the right panel to balance parts, update the preview and export.");
                if studio.bank.is_none(){ui.add_space(12.);ui.label(RichText::new("01  Choose stem files / a Suno ZIP, or load a music mix\n02  Import aligned parts or separate the mix locally\n03  Balance parts, compare, then export").color(theme::AMETHYST));}
                }
            },
            Page::Voice=>{
                let available = !studio.busy();
                ui.add_enabled_ui(available, |ui| {
                    super::tool_inputs::target(studio, ui);
                    super::voice_controls::show(studio, ui, advanced);
                });
            },
            Page::Repair=>{
                let available = !studio.busy();
                ui.add_enabled_ui(available, |ui| {
                    super::tool_inputs::target(studio, ui);
                    super::repair_controls::show(studio, ui, ctx, advanced);
                });
                if studio.spectrogram.is_some() {
                    let graph = ui.scope(|ui| super::spectral_view::show(studio,ui,if compact {180.} else {280.}));
                    // A completed analysis should expose its result once, without
                    // fighting later manual scrolling or changing the graph size.
                    if std::mem::take(&mut studio.reveal_spectrogram) { graph.response.scroll_to_me(Some(egui::Align::Min)); }
                }
            },
            Page::Plugins=>{
                let available = !studio.busy();
                ui.add_enabled_ui(available, |ui| super::plugin_controls::show(studio, ui, ctx));
            },
        }
        if let Some(preview)=&studio.current {
            ui.separator();ui.label(RichText::new(format!("Current preview · {}",preview.label)).color(theme::SLIME));
            for note in &preview.notes {ui.small(note);}
        }
    });
    promote
}
