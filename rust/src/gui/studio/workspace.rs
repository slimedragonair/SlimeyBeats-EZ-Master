use super::{
    spectral_view,
    state::{Page, Studio},
    transport,
};
use crate::gui::{crystal_button, theme};
use eframe::egui::{self, RichText};

/// Returns true when the user explicitly sends the prepared mix to mastering.
pub fn show(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
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
            .clicked()
        {
            studio.history(false);
        }
        if ui
            .add_enabled(
                !studio.busy() && !studio.redo.is_empty(),
                crystal_button::secondary("Redo"),
            )
            .clicked()
        {
            studio.history(true);
        }
        ui.add_enabled(!studio.busy()&&studio.current.is_some()&&!studio.mix_dirty,crystal_button::secondary(if compact {"To master"}else{"Use for mastering"})).on_hover_text("Add a private working copy to the mastering queue. This prepares another stage; it does not export a final delivery file.").clicked()
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
    egui::ScrollArea::vertical().id_salt("studio-workspace").max_height(workspace_height).show(ui,|ui| {
        match studio.page {
            Page::Stems=>{
                if studio.bulk_import.is_some() {
                    ui.label(theme::display("Review your stem set.",if compact {20.} else {24.}));
                    if compact {
                        ui.small("Choose parts and format, then import from the right panel.");
                    } else {
                        ui.label("Each selected part is imported once. Leave the full mix out when importing its individual stems.");
                    }
                    let available = !studio.busy();
                    ui.add_enabled_ui(available, |ui| super::stem_bulk_import::review(studio,ui,ctx,compact));
                } else {
                ui.label(theme::display("Separate. Rebalance. Refine.",24.));
                ui.label("Combine aligned parts with no fixed track count, or separate two/four stems locally. Compare the mix and save when ready.");
                if studio.bank.is_none(){ui.add_space(12.);ui.label(RichText::new("01  Choose stem files / a Suno ZIP, or load a music mix\n02  Import aligned parts or separate the mix locally\n03  Balance parts, compare, then export").color(theme::AMETHYST));}
                for (kind,waveform) in &studio.stem_waveforms {
                    ui.add_space(6.);theme::eyebrow(ui,studio.bank.as_ref().map_or(kind.name(), |bank| bank.display_name(*kind)).as_ref());
                    waveform.show_labeled(ui,0.,studio.source.as_ref().map_or(1.,|s|s.audio.duration()),("STEM",""),76.);
                }
                }
            },
            Page::Voice=>{
                ui.label(theme::display("A cleaner voice, on your terms.",24.));
                ui.label("Work on the original voice, a vocal stem or the current preview. Compare every change; previewing and exporting are separate.");
                ui.add_space(10.);ui.label("Rumble removal · learned noise reduction · de-essing · optional quiet-gap gate");
                ui.small("Change settings in the right panel, then Preview voice cleanup to apply them.");
            },
            Page::Repair=>{
                if !compact {ui.label(theme::display("Refine a precise moment.",24.));}
                spectral_view::show(studio,ui,if compact {workspace_height}else{workspace_height.min(360.)});
            },
            Page::Plugins=>{ui.label(theme::display("Your effects. Your workflow.",24.));ui.label("Choose a trusted VST3 effect, adjust its generic controls, and render a preview. The helper compensates reported latency and retains the source duration.");ui.small("Compatible mono/stereo float32 audio effects are supported. Native vendor editors and MIDI/instrument sequencing are outside this host's scope.");},
        }
        if let Some(preview)=&studio.current {
            ui.separator();ui.label(RichText::new(format!("Current preview · {}",preview.label)).color(theme::SLIME));
            for note in &preview.notes {ui.small(note);}
        }
    });
    promote
}
