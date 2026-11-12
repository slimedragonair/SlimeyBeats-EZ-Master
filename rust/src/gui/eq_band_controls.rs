//! Responsive groups keep every EQ field inside the workspace on narrow windows.
use super::{app::MasterCraftApp, eq_curve::COLORS, help};
use eframe::egui::{self, RichText};

pub fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(10., 8.);
        field_group(ui, 56., |ui| {
            help::label(
                ui,
                "Band",
                "Choose a colored EQ band, or click its numbered node.",
            );
            egui::ComboBox::from_id_salt("eq-band")
                .width(56.)
                .selected_text(
                    RichText::new(format!("{}", app.selected_band + 1))
                        .color(COLORS[app.selected_band]),
                )
                .show_ui(ui, |ui| {
                    for (index, color) in COLORS.iter().enumerate() {
                        ui.selectable_value(
                            &mut app.selected_band,
                            index,
                            RichText::new(format!("Band {}", index + 1)).color(*color),
                        );
                    }
                });
        });
        let band = &mut app.settings.manual_eq[app.selected_band];
        field_group(ui, 42., |ui| {
            let label = help::label(ui, "On", "Enable or bypass only this band.");
            changed |= ui
                .checkbox(&mut band.on, "")
                .labelled_by(label.id)
                .on_hover_text("Enable or bypass this band. Changes are heard during playback.")
                .changed();
        });
        field_group(ui, 96., |ui| {
            help::label(ui, "Shape", help::SHAPE);
            egui::ComboBox::from_id_salt("eq-shape")
                .width(96.)
                .selected_text(band.kind.replace('_', " "))
                .show_ui(ui, |ui| {
                    for kind in ["bell", "low_shelf", "high_shelf", "low_cut", "high_cut"] {
                        changed |= ui
                            .selectable_value(&mut band.kind, kind.into(), kind.replace('_', " "))
                            .changed();
                    }
                })
                .response
                .on_hover_text(help::SHAPE);
        });
        field_group(ui, 88., |ui| {
            help::label(ui, "Frequency", help::FREQUENCY);
            changed |= ui
                .add(
                    egui::DragValue::new(&mut band.freq)
                        .speed(5.)
                        .range(20.0..=20000.)
                        .fixed_decimals(0)
                        .suffix(" Hz"),
                )
                .on_hover_text(help::FREQUENCY)
                .changed();
        });
        field_group(ui, 72., |ui| {
            help::label(ui, "Gain / dB", help::EQ_GAIN);
            changed |= ui
                .add(
                    egui::DragValue::new(&mut band.gain)
                        .speed(0.1)
                        .range(-18.0..=18.0)
                        .fixed_decimals(1),
                )
                .on_hover_text(help::EQ_GAIN)
                .changed();
        });
        field_group(ui, 72., |ui| {
            help::label(ui, "Q / width", help::Q);
            changed |= ui
                .add(
                    egui::DragValue::new(&mut band.q)
                        .speed(0.02)
                        .range(0.1..=18.)
                        .fixed_decimals(2),
                )
                .on_hover_text(help::Q)
                .changed();
        });
    });
    app.eq_dirty |= changed;
}

fn field_group<R>(ui: &mut egui::Ui, width: f32, show: impl FnOnce(&mut egui::Ui) -> R) {
    let height = ui.spacing().interact_size.y
        + ui.text_style_height(&egui::TextStyle::Body)
        + ui.spacing().item_spacing.y;
    // Allocate before painting, so a whole field wraps instead of growing the parent UI.
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(width);
            show(ui)
        },
    );
}
