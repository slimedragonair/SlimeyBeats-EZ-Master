use super::state::Studio;
use crate::gui::theme;
use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};

fn color(db: f32) -> Color32 {
    let t = ((db + 100.) / 85.).clamp(0., 1.);
    let stops = [
        [14., 11., 21.],
        [64., 29., 101.],
        [160., 83., 206.],
        [112., 220., 207.],
        [242., 245., 220.],
    ];
    let p = t * 4.;
    let i = (p as usize).min(3);
    let f = p - i as f32;
    let c = std::array::from_fn::<_, 3, _>(|n| {
        (stops[i][n] * (1. - f) + stops[i + 1][n] * f).round() as u8
    });
    Color32::from_rgb(c[0], c[1], c[2])
}
pub fn show(studio: &mut Studio, ui: &mut egui::Ui, height: f32) {
    let Some(image) = studio.spectrogram.clone() else {
        ui.label("Build the spectrogram to inspect where sounds occur in time and frequency.");
        return;
    };
    if studio.texture.is_none() {
        let mut pixels = vec![Color32::BLACK; image.columns * image.rows];
        for x in 0..image.columns {
            for y in 0..image.rows {
                pixels[(image.rows - 1 - y) * image.columns + x] =
                    color(image.levels_dbfs[x * image.rows + y]);
            }
        }
        studio.texture = Some(ui.ctx().load_texture(
            "studio-spectrogram",
            egui::ColorImage::new([image.columns, image.rows], pixels),
            egui::TextureOptions::LINEAR,
        ));
    }
    ui.label(
        egui::RichText::new("Frequency / Hz · sampled spectrum, dBFS per bin")
            .color(theme::MUTED)
            .size(13.),
    );
    let (outer, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let chart = Rect::from_min_max(outer.min + vec2(48., 8.), outer.max - vec2(10., 30.));
    let response = ui.interact(
        chart,
        ui.scope_id().with("spectral-selection"),
        Sense::click_and_drag(),
    );
    let painter = ui.painter();
    painter.image(
        studio.texture.as_ref().unwrap().id(),
        chart,
        Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)),
        Color32::WHITE,
    );
    let high = *image.frequencies_hz.last().unwrap_or(&24000.);
    let x = |seconds: f64| {
        chart.left()
            + (seconds / image.duration_seconds.max(1e-9)).clamp(0., 1.) as f32 * chart.width()
    };
    let y = |hz: f64| {
        chart.bottom()
            - ((hz.max(20.) / 20.).ln() / (high / 20.).ln()).clamp(0., 1.) as f32 * chart.height()
    };
    for hz in [20., 100., 1000., 10000.] {
        if hz <= high {
            let y = y(hz);
            painter.line_segment(
                [pos2(chart.left(), y), pos2(chart.right(), y)],
                Stroke::new(0.5, theme::BORDER),
            );
            painter.text(
                pos2(chart.left() - 6., y),
                Align2::RIGHT_CENTER,
                format!("{hz:.0}"),
                FontId::proportional(12.),
                theme::MUTED,
            );
        }
    }
    for i in 0..=4 {
        let seconds = image.duration_seconds * i as f64 / 4.;
        painter.text(
            pos2(x(seconds), chart.bottom() + 7.),
            if i == 0 {
                Align2::LEFT_TOP
            } else if i == 4 {
                Align2::RIGHT_TOP
            } else {
                Align2::CENTER_TOP
            },
            format!("{seconds:.1}s"),
            FontId::proportional(12.),
            theme::MUTED,
        );
    }
    if response.drag_started() {
        studio.selection_anchor = response.interact_pointer_pos();
    }
    if response.dragged()
        && let (Some(start), Some(end)) = (studio.selection_anchor, response.interact_pointer_pos())
    {
        let time = |p: egui::Pos2| {
            ((p.x - chart.left()) / chart.width()).clamp(0., 1.) as f64 * image.duration_seconds
        };
        let freq = |p: egui::Pos2| {
            20. * (high / 20.).powf(((chart.bottom() - p.y) / chart.height()).clamp(0., 1.) as f64)
        };
        studio.repair.start_seconds = time(start).min(time(end));
        studio.repair.end_seconds = time(start).max(time(end));
        studio.repair.low_hz = freq(start).min(freq(end));
        studio.repair.high_hz = freq(start).max(freq(end));
    }
    let selected = Rect::from_min_max(
        pos2(x(studio.repair.start_seconds), y(studio.repair.high_hz)),
        pos2(x(studio.repair.end_seconds), y(studio.repair.low_hz)),
    );
    painter.rect_filled(selected, 0., theme::AMETHYST.gamma_multiply(0.12));
    painter.rect_stroke(
        selected,
        0.,
        Stroke::new(1.5, theme::AMETHYST),
        egui::StrokeKind::Inside,
    );
    ui.small("Dark: quiet · violet: detail · teal: stronger energy. A bright region is not automatically a defect.");
}
