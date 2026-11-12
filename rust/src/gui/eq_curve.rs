use super::{app::MasterCraftApp, live_spectrum, plots, theme};
use crate::dsp::biquad;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke};
use std::sync::atomic::Ordering;

pub const COLORS: [Color32; 7] = [
    Color32::from_rgb(197, 148, 255),
    Color32::from_rgb(237, 142, 182),
    Color32::from_rgb(241, 168, 114),
    Color32::from_rgb(226, 209, 153),
    Color32::from_rgb(151, 215, 174),
    Color32::from_rgb(114, 220, 209),
    Color32::from_rgb(151, 180, 244),
];

pub fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui, sr: u32) {
    ui.horizontal_wrapped(|ui| {
        for (label, color) in [
            ("EQ response", theme::AMETHYST),
            ("Pre EQ spectrum", theme::MUTED),
            ("EQ output spectrum", theme::SLIME),
        ] {
            ui.label(RichText::new(label).size(12.).color(color));
        }
        if app.suggested_eq.is_some() {
            let label = if app.suggested_eq.as_ref() == Some(&app.settings.manual_eq) {
                "Proposed EQ · applied"
            } else {
                "Proposed EQ · not applied"
            };
            ui.label(RichText::new(label).size(12.).color(theme::BONE));
        }
    });
    let mode = if app.player.controls.playing.load(Ordering::Relaxed) {
        "Playing · live output"
    } else if app.player.seconds() >= 1. / sr as f64 {
        "Paused · playhead window"
    } else if app.eq_dirty {
        "Paused · last measured average"
    } else {
        "Paused · full-track average"
    };
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(
            ui.available_width(),
            (ui.ctx().viewport_rect().height() - 790.).clamp(140., 270.),
        ),
        Sense::click_and_drag(),
    );
    let stacked_labels = rect.width() - 72. < 430.;
    let chart = Rect::from_min_max(
        rect.min + egui::vec2(32., if stacked_labels { 39. } else { 24. }),
        rect.max - egui::vec2(40., 25.),
    );
    for (position, alignment, label, color) in [
        (
            egui::pos2(chart.left(), rect.top()),
            Align2::LEFT_TOP,
            "EQ gain · dB",
            theme::AMETHYST,
        ),
        (
            egui::pos2(
                chart.center().x,
                rect.top() + if stacked_labels { 15. } else { 0. },
            ),
            Align2::CENTER_TOP,
            mode,
            theme::MUTED,
        ),
        (
            egui::pos2(chart.right(), rect.top()),
            Align2::RIGHT_TOP,
            "Spectrum · dBFS/bin",
            theme::SLIME,
        ),
    ] {
        ui.painter()
            .text(position, alignment, label, FontId::proportional(12.), color);
    }
    plots::grid(ui, chart, -18., 18.);
    let result = app.current().map(|(_, r)| r);
    live_spectrum::paint(app, ui, chart, result.as_deref());
    for value in [-90., -60., -30., 0.] {
        ui.painter().text(
            Pos2::new(chart.right() + 5., plots::value_y(value, chart, -90., 0.)),
            Align2::LEFT_CENTER,
            format!("{value:.0}"),
            FontId::proportional(12.),
            theme::SLIME,
        );
    }
    let freqs: Vec<_> = (0..240)
        .map(|i| 20. * 1000_f64.powf(i as f64 / 239.))
        .collect();
    if let Some(suggested) = &app.suggested_eq {
        let values: Vec<_> = freqs
            .iter()
            .map(|&f| {
                suggested
                    .iter()
                    .filter(|b| b.on)
                    .map(|b| biquad::from_band(b, sr).response_db(f, sr))
                    .sum()
            })
            .collect();
        plots::curve(
            ui,
            chart,
            &freqs,
            &values,
            -18.,
            18.,
            Stroke::new(1.4, theme::BONE),
        );
    }
    let values: Vec<_> = freqs
        .iter()
        .map(|&f| {
            app.settings
                .manual_eq
                .iter()
                .filter(|b| b.on)
                .map(|b| biquad::from_band(b, sr).response_db(f, sr))
                .sum()
        })
        .collect();
    plots::curve(
        ui,
        chart,
        &freqs,
        &values,
        -18.,
        18.,
        Stroke::new(2., theme::AMETHYST),
    );
    for (i, band) in app.settings.manual_eq.iter().enumerate() {
        let point = Pos2::new(
            plots::frequency_x(band.freq, chart),
            plots::value_y(band.gain, chart, -18., 18.),
        );
        ui.painter().circle_filled(
            point,
            if i == app.selected_band { 9. } else { 7. },
            if band.on { COLORS[i] } else { theme::MUTED },
        );
        ui.painter().text(
            point,
            Align2::CENTER_CENTER,
            format!("{}", i + 1),
            FontId::proportional(11.),
            theme::CANVAS,
        );
    }
    if (response.drag_started() || response.clicked())
        && let Some(pos) = response.interact_pointer_pos()
    {
        app.selected_band = app
            .settings
            .manual_eq
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let point = |band: &crate::config::EqBand| {
                    Pos2::new(
                        plots::frequency_x(band.freq, chart),
                        plots::value_y(band.gain, chart, -18., 18.),
                    )
                };
                point(a)
                    .distance_sq(pos)
                    .total_cmp(&point(b).distance_sq(pos))
            })
            .map(|(i, _)| i)
            .unwrap_or(0);
    }
    if response.dragged()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let band = &mut app.settings.manual_eq[app.selected_band];
        band.freq = (20.
            * 1000_f64.powf(((pos.x - chart.left()) / chart.width()).clamp(0., 1.) as f64))
        .min(sr as f64 * 0.49);
        band.gain = (18. - 36. * ((pos.y - chart.top()) / chart.height()) as f64).clamp(-18., 18.);
        app.eq_dirty = true;
    }
    response.on_hover_text("Drag a numbered node left/right to choose a frequency, up/down to boost or cut. Purple is your editable EQ response. Bone-white is the suggested curve, if available. Edits affect B immediately during playback. Left axis is EQ gain; right axis is audio level. Gray is pre-EQ; teal is the live EQ signal while playing. At the paused start, the spectrum shows the measured full-track average.");
}
