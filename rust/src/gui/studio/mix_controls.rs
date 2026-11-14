//! Aligned stem rows: compact controls first, with optional readable waveforms.
use super::state::Studio;
use crate::{
    gui::{control_layout::bounded_slider, theme},
    studio::stem_mix::StemControl,
};
use eframe::egui::{self, RichText};

pub(super) fn show(studio: &mut Studio, ui: &mut egui::Ui, advanced: bool, compact: bool) {
    let Some(bank) = &studio.bank else {
        return;
    };
    ui.small(format!(
        "{} parts · levels, mute and solo update live",
        bank.tracks.len()
    ));
    let graph_id = ui.make_persistent_id("studio-part-waveforms");
    let mut graphs = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(graph_id))
        .unwrap_or(!compact);
    if ui.checkbox(&mut graphs, "Show part waveforms").on_hover_text("Display full-height plots beside each part. Hiding them changes no audio or mix settings.").changed() {
        ui.ctx().data_mut(|data| data.insert_temp(graph_id, graphs));
    }
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
            let track_id =
                ui.make_persistent_id(egui::IdSalt::new(("studio-fader", track.kind.name())));
            ui.scope_builder(egui::UiBuilder::new().scope_id(track_id), |ui| {
                egui::Frame::group(ui.style())
                    .inner_margin(if compact { 5 } else { 8 })
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = if compact { 3. } else { 7. };
                        let name = bank.display_name(track.kind);
                        if compact && ui.available_width() >= 540. {
                            ui.horizontal(|ui| {
                                let name_width = (ui.available_width() * 0.25).clamp(115., 200.);
                                ui.add_sized(
                                    [name_width, ui.spacing().interact_size.y],
                                    egui::Label::new(
                                        RichText::new(name.as_ref())
                                            .strong()
                                            .color(theme::AMETHYST),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(name.as_ref());
                                changed |= gain(track, ui, name.as_ref(), 250.);
                                changed |= switches(track, ui, name.as_ref());
                            });
                            if advanced && stereo {
                                changed |= balance(track, ui, name.as_ref());
                            }
                        } else {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(name.as_ref()).strong().color(theme::AMETHYST),
                                )
                                .truncate(),
                            )
                            .on_hover_text(name.as_ref());
                            ui.horizontal_wrapped(|ui| {
                                changed |= gain(track, ui, name.as_ref(), 320.);
                                changed |= switches(track, ui, name.as_ref());
                            });
                            if advanced && stereo {
                                changed |= balance(track, ui, name.as_ref());
                            }
                        }
                        if graphs
                            && let Some((_, waveform)) =
                                waveforms.iter().find(|(kind, _)| *kind == track.kind)
                        {
                            waveform.show_labeled(ui, 0., duration, ("PART", ""), 104.);
                        }
                    });
            });
        }
    });
    if changed {
        studio.monitor_mix_changes();
    }
}

fn gain(track: &mut StemControl, ui: &mut egui::Ui, name: &str, max_width: f32) -> bool {
    // A slider is a rail plus a numeric editor. egui propagates this accessible
    // name to the other half even when the numeric editor takes keyboard focus.
    ui.scope(|ui| {
        ui.set_width(ui.available_width().min(max_width));
        bounded_slider(ui, &mut track.gain_db, -60. ..=12., " dB", None)
            .on_hover_text(format!(
                "{name} gain, dB. Hear changes live; update mix preview before saving."
            ))
            .changed()
    })
    .inner
}

fn switches(track: &mut StemControl, ui: &mut egui::Ui, name: &str) -> bool {
    let mute = ui
        .checkbox(&mut track.mute, "Mute")
        .on_hover_text(format!("Mute {name}. The bank audio stays intact."))
        .changed();
    let solo = ui
        .checkbox(&mut track.solo, "Solo")
        .on_hover_text(format!("Solo {name}. Muted parts remain muted."))
        .changed();
    mute || solo
}

fn balance(track: &mut StemControl, ui: &mut egui::Ui, name: &str) -> bool {
    ui.horizontal_wrapped(|ui| {
        ui.label("Stereo balance");
        ui.scope(|ui| {
            ui.set_width(ui.available_width().min(250.));
            bounded_slider(ui, &mut track.balance, -1. ..=1., "", None)
                .on_hover_text(format!(
                    "{name} stereo balance. -1 keeps left, +1 keeps right, 0 leaves both unchanged."
                ))
                .changed()
        })
        .inner
    })
    .inner
}
