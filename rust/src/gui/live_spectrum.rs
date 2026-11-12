use super::{app::MasterCraftApp, plots, theme, worker::TrackResult};
use eframe::egui::{self, Rect, Stroke};
use std::sync::atomic::Ordering;

pub fn paint(app: &mut MasterCraftApp, ui: &egui::Ui, chart: Rect, result: Option<&TrackResult>) {
    let Some(result) = result else {
        return;
    };
    let before = result.raw.as_deref().unwrap_or(&result.before);
    let playing = app.player.controls.playing.load(Ordering::Relaxed);
    if !playing && app.player.seconds() < 1. / before.sample_rate as f64 {
        let average = &result
            .raw_analysis
            .as_deref()
            .unwrap_or(&result.analysis)
            .spectrum;
        series(
            ui,
            chart,
            &average.freqs,
            &average.db,
            Stroke::new(1., theme::MUTED),
            theme::MUTED.linear_multiply(0.15),
        );
        if let Some(report) = &result.report {
            series(
                ui,
                chart,
                &report.output.spectrum.freqs,
                &report.output.spectrum.db,
                Stroke::new(1.5, theme::SLIME),
                theme::SLIME.linear_multiply(0.08),
            );
        }
        return;
    }
    let tap = if playing {
        app.player.tap.snapshot(before.sample_rate, before.channels)
    } else {
        None
    };
    app.playback_spectrum.update_live(
        before,
        result.after.as_deref(),
        app.player.seconds(),
        tap.as_ref(),
    );
    let spectrum = &app.playback_spectrum;
    series(
        ui,
        chart,
        &spectrum.frequencies,
        &spectrum.before_db,
        Stroke::new(1., theme::MUTED),
        theme::MUTED.linear_multiply(0.15),
    );
    if !spectrum.after_db.is_empty() {
        series(
            ui,
            chart,
            &spectrum.frequencies,
            &spectrum.after_db,
            Stroke::new(1.5, theme::SLIME),
            theme::SLIME.linear_multiply(0.08),
        );
    }
}
fn series(
    ui: &egui::Ui,
    chart: Rect,
    frequencies: &[f64],
    values: &[f64],
    stroke: Stroke,
    fill: egui::Color32,
) {
    shade(ui, chart, frequencies, values, fill);
    plots::curve(ui, chart, frequencies, values, -90., 0., stroke);
}
fn shade(ui: &egui::Ui, rect: Rect, frequencies: &[f64], values: &[f64], color: egui::Color32) {
    let mut mesh = egui::Mesh::default();
    for (&frequency, &value) in frequencies.iter().zip(values) {
        let x = plots::frequency_x(frequency, rect);
        let y = plots::value_y(value.clamp(-90., 0.), rect, -90., 0.);
        mesh.colored_vertex(egui::pos2(x, y), color);
        mesh.colored_vertex(egui::pos2(x, rect.bottom()), color);
    }
    for i in 1..frequencies.len().min(values.len()) {
        let a = ((i - 1) * 2) as u32;
        let b = (i * 2) as u32;
        mesh.add_triangle(a, a + 1, b);
        mesh.add_triangle(b, a + 1, b + 1);
    }
    ui.painter()
        .with_clip_rect(rect)
        .add(egui::Shape::mesh(mesh));
}
