//! Exact repair selection and display preparation, separate from rendering/export.
use super::{state::Studio, tool_fields::slider};
use crate::{
    gui::{crystal_button, theme},
    studio::spectral_repair::RepairMode,
};
use eframe::egui;

pub(super) fn show(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context, advanced: bool) {
    if ui
        .add_enabled(
            studio.audio_target().is_ok(),
            crystal_button::secondary("Build spectrogram"),
        )
        .clicked()
    {
        studio.build_spectrogram(ctx);
    }
    ui.separator();
    theme::eyebrow(ui, "TIME / FREQUENCY SELECTION");
    ui.label(
        "Select an unwanted sound by time and pitch. Drag the graph or type exact values below.",
    );
    if !advanced && studio.repair.mode != RepairMode::Attenuate {
        ui.colored_label(
            theme::BONE,
            "Custom repair mode active · Heal from surroundings. Advanced shows the mode control.",
        );
    }
    egui::Grid::new("studio-repair-region")
        .num_columns(4)
        .spacing([10., 5.])
        .show(ui, |ui| {
            for (index, (label, value, speed, suffix)) in [
                ("Start", &mut studio.repair.start_seconds, 0.01, " s"),
                ("End", &mut studio.repair.end_seconds, 0.01, " s"),
                ("Low", &mut studio.repair.low_hz, 10., " Hz"),
                ("High", &mut studio.repair.high_hz, 10., " Hz"),
            ]
            .into_iter()
            .enumerate()
            {
                let label = ui.label(label);
                ui.add(egui::DragValue::new(value).speed(speed).suffix(suffix))
                    .labelled_by(label.id);
                if index % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    slider(
        ui,
        "Reduction",
        &mut studio.repair.reduction_db,
        -60. ..=0.,
        " dB",
        "How much to reduce the selected region. -6 dB is roughly half amplitude; 0 is an exact bypass.",
    );
    if advanced {
        ui.radio_value(
            &mut studio.repair.mode,
            RepairMode::Attenuate,
            "Turn down selected sound",
        )
        .on_hover_text("Turn down the selected time/frequency region with soft boundaries.");
        ui.radio_value(&mut studio.repair.mode,RepairMode::Heal,"Heal from surroundings").on_hover_text("Estimate surrounding spectral magnitudes to soften a localized burst. It needs unselected time context and cannot recreate missing words.");
    }
}
