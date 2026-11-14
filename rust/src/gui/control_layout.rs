use super::crystal_slider::CrystalSlider;
use eframe::egui;
use std::ops::RangeInclusive;

pub(super) fn action_row<R>(
    ui: &mut egui::Ui,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.horizontal_wrapped(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        add(ui)
    })
}

pub(super) fn section_header(ui: &egui::Ui, text: &str) -> egui::CollapsingHeader {
    // CollapsingHeader otherwise forces Extend, even in a wrapping settings pane.
    let width =
        (ui.available_width() - ui.spacing().indent - ui.spacing().button_padding.x).max(1.);
    let text = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        width,
        egui::TextStyle::Button,
    );
    egui::CollapsingHeader::new(text)
}

fn configured_slider<'a>(
    value: &'a mut f64,
    range: RangeInclusive<f64>,
    suffix: &'a str,
    decimals: Option<usize>,
) -> CrystalSlider<'a> {
    let slider = CrystalSlider::new(value, range).suffix(suffix);
    if let Some(decimals) = decimals {
        slider.fixed_decimals(decimals)
    } else {
        slider
    }
}

pub(super) fn bounded_slider(
    ui: &mut egui::Ui,
    value: &mut f64,
    range: RangeInclusive<f64>,
    suffix: &str,
    decimals: Option<usize>,
) -> egui::Response {
    let width = ui.available_width();
    // Slider's response includes its native DragValue. Measure that actual overhead
    // on a disabled, invisible copy: suffixes, fonts and numeric precision all affect it.
    // A fixed allowance can exceed the pane by a few pixels and grow it every repaint.
    let mut measured_value = *value;
    let mut measure = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("slider-width-measure")
            .sizing_pass()
            .invisible(),
    );
    measure.spacing_mut().slider_width = width;
    let measured_width = measure
        .add(configured_slider(
            &mut measured_value,
            range.clone(),
            suffix,
            decimals,
        ))
        .rect
        .width();
    let value_width = (measured_width - width).max(0.);
    ui.scope(|ui| {
        // Leave one physical pixel for independent layout rounding in the scroll area.
        ui.spacing_mut().slider_width = (width - value_width - 1. / ui.pixels_per_point()).max(1.);
        ui.add(configured_slider(value, range, suffix, decimals))
    })
    .inner
}
