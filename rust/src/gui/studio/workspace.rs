use super::{
    spectral_view,
    state::{Page, Studio},
    transport,
};
use crate::{
    gui::{crystal_button, crystal_slider::CrystalSlider, theme},
    studio::stem_mix::StemControl,
};
use eframe::egui::{self, RichText};

/// Returns true when the user explicitly sends the prepared mix to mastering.
pub fn show(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context, advanced: bool) -> bool {
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
    egui::ScrollArea::vertical().id_salt("studio-workspace").max_height(workspace_height).show(ui,|ui| {
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
                    ui.label(theme::display("Mix your stems.", if compact {20.} else {24.}));
                    ui.small("Adjust parts here → Update mix preview → compare A/B → Export.");
                    mixer(studio, ui, advanced, compact);
                } else {
                ui.label(theme::display("Separate. Rebalance. Refine.",24.));
                ui.label("Import existing stems with no model download. Use the right panel to balance parts, update the preview and export.");
                if studio.bank.is_none(){ui.add_space(12.);ui.label(RichText::new("01  Choose stem files / a Suno ZIP, or load a music mix\n02  Import aligned parts or separate the mix locally\n03  Balance parts, compare, then export").color(theme::AMETHYST));}
                }
            },
            Page::Voice=>{
                ui.label(theme::display("A cleaner voice, on your terms.",24.));
                ui.label("Work on the original voice, a vocal stem or the current preview. Compare every change; previewing and exporting are separate.");
                ui.add_space(10.);ui.label("Rumble removal · learned noise reduction · de-essing · optional quiet-gap gate");
                ui.small("1. Choose the input and settings. 2. Preview voice cleanup to compare A/B. 3. Export the prepared audio. A stem edit can be applied back into the mix first.");
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

fn mixer(studio: &mut Studio, ui: &mut egui::Ui, advanced: bool, compact: bool) {
    let Some(bank) = &studio.bank else {
        return;
    };
    ui.small(format!(
        "{} parts · scroll to view all parts and their controls",
        bank.tracks.len()
    ));
    let custom_balance = studio
        .mix
        .tracks
        .iter()
        .filter(|track| track.balance != 0.)
        .count();
    if !advanced && custom_balance > 0 {
        ui.colored_label(theme::BONE, format!("Custom balance active on {custom_balance} part(s). Advanced shows balance controls."));
    }
    if studio.mix.output_gain_db != 0.
        || !studio.mix.protect_peaks
        || studio.mix.ceiling_dbtp != -1.
    {
        ui.small(format!(
            "Project mix output: {:+.1} dB · peak protection {} · {:.1} dBTP ceiling",
            studio.mix.output_gain_db,
            if studio.mix.protect_peaks {
                "on"
            } else {
                "off"
            },
            studio.mix.ceiling_dbtp
        ));
    }
    let available = !studio.busy();
    let bank = studio.bank.as_ref().unwrap();
    let stereo = bank.tracks[0].audio.channels == 2;
    let duration = bank.tracks[0].audio.duration();
    let waveforms = &studio.stem_waveforms;
    let mut changed = false;
    ui.add_enabled_ui(available, |ui| {
        for track in &mut studio.mix.tracks {
            // Keep keyboard focus when a pending notice changes preceding automatic IDs.
            let track_id =
                ui.make_persistent_id(egui::IdSalt::new(("studio-fader", track.kind.name())));
            ui.scope_builder(egui::UiBuilder::new().scope_id(track_id), |ui| {
                egui::Frame::group(ui.style())
                    .inner_margin(if compact { 6 } else { 8 })
                    .show(ui, |ui| {
                        let name = bank.display_name(track.kind);
                        let waveform = waveforms
                            .iter()
                            .find(|(kind, _)| *kind == track.kind)
                            .map(|(_, waveform)| waveform);
                        if ui.available_width() >= 560. {
                            ui.columns(2, |columns| {
                                columns[0]
                                    .add(
                                        egui::Label::new(
                                            RichText::new(name.as_ref())
                                                .strong()
                                                .color(theme::AMETHYST),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(name.as_ref());
                                if let Some(waveform) = waveform {
                                    waveform.show_labeled(
                                        &mut columns[0],
                                        0.,
                                        duration,
                                        ("PART", ""),
                                        if compact { 48. } else { 58. },
                                    );
                                }
                                changed |= part_controls(track, &mut columns[1], advanced, stereo);
                            });
                        } else {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(name.as_ref()).strong().color(theme::AMETHYST),
                                )
                                .truncate(),
                            )
                            .on_hover_text(name.as_ref());
                            if let Some(waveform) = waveform {
                                waveform.show_labeled(ui, 0., duration, ("PART", ""), 48.);
                            }
                            changed |= part_controls(track, ui, advanced, stereo);
                        }
                    });
            });
        }
    });
    studio.mix_dirty |= changed;
}

fn part_controls(track: &mut StemControl, ui: &mut egui::Ui, advanced: bool, stereo: bool) -> bool {
    ui.spacing_mut().slider_width = (ui.available_width() - 85.).clamp(70., 180.);
    let mut changed = ui
        .add(CrystalSlider::new(&mut track.gain_db, -60. ..=12.).suffix(" dB"))
        .on_hover_text(
            "Stem level: lower or raise this part, then Update mix preview to hear the change.",
        )
        .changed();
    ui.horizontal_wrapped(|ui| {
        changed |= ui
            .checkbox(&mut track.mute, "Mute")
            .on_hover_text("Exclude this part from the mix. Its bank audio is unchanged.")
            .changed();
        changed |= ui
            .checkbox(&mut track.solo, "Solo")
            .on_hover_text("Hear only soloed parts. A muted part remains muted.")
            .changed();
    });
    if advanced && stereo {
        ui.label("Stereo balance");
        changed |= ui
            .add(CrystalSlider::new(&mut track.balance, -1. ..=1.))
            .on_hover_text("-1 keeps the left side, +1 keeps the right; 0 leaves both unchanged.")
            .changed();
    }
    changed
}
