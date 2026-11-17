//! A single pinned transport for the current clip recipe and its initial-import reference.
use super::state::Studio;
use crate::gui::{crystal_button, crystal_slider::CrystalSlider, theme};
use eframe::egui;
use std::sync::atomic::Ordering;

pub(super) fn dock(studio: &mut Studio, ui: &mut egui::Ui) {
    egui::Panel::bottom("arrangement-listening").frame(egui::Frame::NONE).show(ui, |ui| {
        ui.separator();
        ui.add_enabled_ui(studio.player.has_live_arrangement(), |ui| {
            ui.horizontal_wrapped(|ui| {
                let playing = studio.player.controls.playing.load(Ordering::Relaxed);
                if ui.add(crystal_button::secondary(if playing { "Pause" } else { "Play" })).clicked() {
                    if studio.player.seconds() >= studio.player.duration { studio.player.seek(0.); }
                    studio.player.toggle();
                }
                if ui.add(crystal_button::secondary("Stop")).clicked() { studio.player.stop(); }
                let after = studio.player.controls.after.load(Ordering::Relaxed);
                if ui.add(crystal_button::secondary("A · Initial import").selected(!after))
                    .on_hover_text("Compare with the first imported session, before later clip edits. Use the target EQ's Bypass control for an EQ-only comparison.").clicked() {
                    studio.player.controls.after.store(false, Ordering::Relaxed);
                }
                if ui.add(crystal_button::secondary("B · Current tracks").selected(after))
                    .on_hover_text("Hear the current positions, trims, track controls and scoped EQ. A and B are never layered together.").clicked() {
                    studio.player.controls.after.store(true, Ordering::Relaxed);
                }
                ui.label(format!("{:.2} / {:.2} s", studio.player.seconds(), studio.player.duration));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Listening volume");
                ui.add(CrystalSlider::new(&mut studio.volume, 0. ..=1.).show_value(false));
                if studio.player.arrangement_preparing() { ui.spinner(); ui.small("Preparing latest clip edit…"); }
                else { ui.colored_label(theme::SLIME, "Live tracks"); }
            });
        });
        if studio.busy() {
            ui.horizontal_wrapped(|ui| {
                ui.spinner(); ui.add(egui::ProgressBar::new(studio.progress).desired_width(130.));
                if ui.add(crystal_button::secondary("Cancel")).clicked() { studio.cancel(); }
            });
        }
        if ui.ctx().viewport_rect().height() >= 800. {
            ui.add(egui::Label::new(egui::RichText::new(&studio.status).small().color(theme::MUTED)).wrap());
        }
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
        .store(false, Ordering::Relaxed);
}
