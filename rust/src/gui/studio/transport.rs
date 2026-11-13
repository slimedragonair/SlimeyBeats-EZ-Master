use super::state::Studio;
use crate::gui::{crystal_button, crystal_slider::CrystalSlider, theme};
use eframe::egui;
use std::sync::atomic::Ordering;

pub fn show(studio: &mut Studio, ui: &mut egui::Ui) {
    ui.add_enabled_ui(studio.current.is_some(),|ui| {
        ui.horizontal_wrapped(|ui| {
            let playing=studio.player.controls.playing.load(Ordering::Relaxed);
            if ui.add(crystal_button::secondary(if playing{"Pause"}else{"Play"})).clicked() {if studio.player.seconds()>=studio.player.duration {studio.player.seek(0.);}studio.player.toggle();}
            if ui.add(crystal_button::secondary("Stop")).clicked() {studio.player.stop();}
            if ui.add(crystal_button::secondary("A · Before").selected(!studio.player.controls.after.load(Ordering::Relaxed))).on_hover_text("Compare with the input used for this revision, without changing saved samples.").clicked() {studio.player.controls.after.store(false,Ordering::Relaxed);}
            if ui.add(crystal_button::secondary("B · Preview").selected(studio.player.controls.after.load(Ordering::Relaxed))).clicked() {studio.player.controls.after.store(true,Ordering::Relaxed);}
        });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut studio.match_levels,"Level match").on_hover_text("Listening-only loudness matching. It never alters the saved Studio audio.");
            ui.label("Volume").on_hover_text("Listening volume only; saved samples are unchanged.");
            ui.spacing_mut().slider_width=90.;
            ui.add(CrystalSlider::new(&mut studio.volume,0. ..=1.).show_value(false));
            ui.label(egui::RichText::new(format!("{:.1} / {:.1} s",studio.player.seconds(),studio.player.duration)).color(theme::MUTED));
        });
    });
    studio
        .player
        .controls
        .volume
        .store(studio.volume.to_bits(), Ordering::Relaxed);
    studio
        .player
        .controls
        .matched
        .store(studio.match_levels, Ordering::Relaxed);
}
