//! Exact second/frame conversion and stable typed drafts for timeline inspectors.
use anyhow::{Result, ensure};
use eframe::egui;

pub(super) fn seconds(ui: &mut egui::Ui, id: &str, name: &str, value: &mut f64, tip: &str) -> bool {
    ui.label(name).on_hover_text(tip);
    let response = ui
        .push_id(id, |ui| {
            ui.add(
                egui::DragValue::new(value)
                    .speed(0.01)
                    .suffix(" s")
                    .max_decimals(6)
                    .update_while_editing(false),
            )
            .on_hover_text(tip)
        })
        .inner;
    ui.end_row();
    response.changed()
}

pub(super) fn frame(seconds: f64, rate: u32) -> Result<u64> {
    ensure!(
        seconds.is_finite() && seconds >= 0. && rate > 0,
        "Enter finite, nonnegative seconds"
    );
    let value = (seconds * rate as f64).round();
    ensure!(
        value <= crate::studio::arrangement::MAX_FRAME as f64,
        "Time is beyond the supported project range"
    );
    Ok(value as u64)
}
