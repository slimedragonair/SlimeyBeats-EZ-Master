//! Shared labelled numeric sliders for the Studio tool editors.
use crate::gui::control_layout::bounded_slider;
use eframe::egui;

pub(super) fn slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
    help: &str,
) {
    let label = ui.label(label).on_hover_text(help);
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(350.));
        bounded_slider(ui, value, range, suffix, None)
            .labelled_by(label.id)
            .on_hover_text(help);
    });
}
