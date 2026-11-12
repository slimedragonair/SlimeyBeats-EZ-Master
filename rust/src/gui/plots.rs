use super::theme::{BONE, BORDER, MUTED, SLIME};
use crate::{analysis::Spectrum, audio::Audio};
use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Stroke};

pub fn spectrum(
    ui: &mut egui::Ui,
    before: Option<&Spectrum>,
    after: Option<&Spectrum>,
    height: f32,
) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::hover());
    let chart = Rect::from_min_max(
        rect.min + egui::vec2(42., 12.),
        rect.max - egui::vec2(16., 25.),
    );
    grid(ui, chart, -90., -10.);
    if let Some(before) = before {
        curve(
            ui,
            chart,
            &before.freqs,
            &before.db,
            -90.,
            -10.,
            Stroke::new(1.2, MUTED),
        );
    }
    if let Some(after) = after {
        curve(
            ui,
            chart,
            &after.freqs,
            &after.db,
            -90.,
            -10.,
            Stroke::new(2., SLIME),
        );
    }
    if before.is_none() {
        ui.painter().text(
            chart.center(),
            Align2::CENTER_CENTER,
            "Analyze a track to see its spectrum",
            FontId::proportional(16.),
            MUTED,
        );
    }
}
pub fn grid(ui: &egui::Ui, chart: Rect, low: f64, high: f64) {
    let p = ui.painter();
    p.rect_filled(chart, 3, super::theme::CANVAS);
    for f in [
        20., 50., 100., 200., 500., 1000., 2000., 5000., 10000., 20000.,
    ] {
        let x = frequency_x(f, chart);
        p.line_segment(
            [Pos2::new(x, chart.top()), Pos2::new(x, chart.bottom())],
            Stroke::new(0.5, BORDER),
        );
        let label = if f >= 1000. {
            format!("{}k", f / 1000.)
        } else {
            format!("{f:.0}")
        };
        if [20., 100., 1000., 10000.].contains(&f) {
            p.text(
                Pos2::new(x, chart.bottom() + 7.),
                Align2::CENTER_TOP,
                label,
                FontId::proportional(12.),
                MUTED,
            );
        }
    }
    let step = if high - low > 40. { 20 } else { 6 };
    let mut db = (low as i32 / step) * step;
    while db <= high as i32 {
        let y = value_y(db as f64, chart, low, high);
        p.line_segment(
            [Pos2::new(chart.left(), y), Pos2::new(chart.right(), y)],
            Stroke::new(if db == 0 { 1. } else { 0.5 }, BORDER),
        );
        p.text(
            Pos2::new(chart.left() - 8., y),
            Align2::RIGHT_CENTER,
            format!("{db}"),
            FontId::proportional(12.),
            MUTED,
        );
        db += step;
    }
}
pub fn frequency_x(f: f64, rect: Rect) -> f32 {
    rect.left() + ((f.max(20.) / 20.).log10() / 3.) as f32 * rect.width()
}
pub fn value_y(v: f64, rect: Rect, lo: f64, hi: f64) -> f32 {
    rect.bottom() - ((v - lo) / (hi - lo)) as f32 * rect.height()
}
pub fn curve(
    ui: &egui::Ui,
    rect: Rect,
    freqs: &[f64],
    values: &[f64],
    lo: f64,
    hi: f64,
    stroke: Stroke,
) {
    let points: Vec<_> = freqs
        .iter()
        .zip(values)
        .map(|(&f, &v)| Pos2::new(frequency_x(f, rect), value_y(v.clamp(lo, hi), rect, lo, hi)))
        .collect();
    ui.painter()
        .with_clip_rect(rect)
        .add(egui::Shape::line(points, stroke));
}

#[derive(Default)]
pub struct Waveform {
    pub before: Vec<f32>,
    pub after: Vec<f32>,
}
impl Waveform {
    pub fn new(before: &Audio, after: Option<&Audio>) -> Self {
        Self {
            before: peaks(before),
            after: after.map(peaks).unwrap_or_default(),
        }
    }
    pub fn show(&self, ui: &mut egui::Ui, seconds: f64, duration: f64) -> Option<f64> {
        self.show_labeled(ui, seconds, duration, ("BEFORE", "AFTER"), 176.)
    }
    pub fn show_labeled(
        &self,
        ui: &mut egui::Ui,
        seconds: f64,
        duration: f64,
        labels: (&str, &str),
        height: f32,
    ) -> Option<f64> {
        let font = FontId::proportional(12.);
        let texts = [
            ui.painter()
                .layout_no_wrap(labels.0.into(), font.clone(), MUTED),
            ui.painter().layout_no_wrap(labels.1.into(), font, SLIME),
        ];
        let label_height = texts.iter().map(|text| text.size().y).fold(0_f32, f32::max);
        let height = height.max(label_height * 3. + 24.);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            Sense::click_and_drag(),
        );
        let p = ui.painter();
        p.rect_filled(rect, 3, super::theme::CANVAS);
        let layout = waveform_layout(rect, label_height);
        let plot = layout.plot;
        let playhead = plot.left() + seconds as f32 / duration.max(0.001) as f32 * plot.width();
        for (index, (data, color)) in [(&self.before, MUTED), (&self.after, SLIME)]
            .into_iter()
            .enumerate()
        {
            let lane = layout.traces[index];
            for (i, &peak) in data.iter().enumerate() {
                let x = lane.left() + i as f32 / data.len().max(1) as f32 * lane.width();
                let half = peak.min(1.) * 24_f32.min(lane.height() * 0.5);
                p.line_segment(
                    [
                        Pos2::new(x, lane.center().y - half),
                        Pos2::new(x, lane.center().y + half),
                    ],
                    Stroke::new(1., color),
                );
            }
            p.line_segment(
                [
                    Pos2::new(playhead, lane.top()),
                    Pos2::new(playhead, lane.bottom()),
                ],
                Stroke::new(1., BONE),
            );
            p.galley(layout.labels[index], texts[index].clone(), color);
        }
        for i in 0..=4 {
            let x = plot.left() + plot.width() * i as f32 / 4.;
            let t = duration * i as f64 / 4.;
            p.text(
                Pos2::new(x, rect.bottom() - 5.),
                if i == 0 {
                    Align2::LEFT_BOTTOM
                } else if i == 4 {
                    Align2::RIGHT_BOTTOM
                } else {
                    Align2::CENTER_BOTTOM
                },
                format!("{}:{:02}", t as u64 / 60, t as u64 % 60),
                FontId::proportional(12.),
                MUTED,
            );
        }
        if response.clicked() || response.dragged() {
            response
                .interact_pointer_pos()
                .map(|p| ((p.x - plot.left()) / plot.width()).clamp(0., 1.) as f64 * duration)
        } else {
            None
        }
    }
}
fn peaks(audio: &Audio) -> Vec<f32> {
    let width = 1600;
    let block = audio.frames().div_ceil(width).max(1) * audio.channels;
    audio
        .samples
        .chunks(block)
        .map(|b| b.iter().map(|v| v.abs()).fold(0_f32, f32::max))
        .collect()
}
struct WaveformLayout {
    plot: Rect,
    traces: [Rect; 2],
    labels: [Pos2; 2],
}
fn waveform_layout(rect: Rect, label_height: f32) -> WaveformLayout {
    let plot = Rect::from_min_max(
        rect.min + egui::vec2(12., 0.),
        rect.max - egui::vec2(12., label_height + 7.),
    );
    let lane_height = plot.height() * 0.5;
    let labels = std::array::from_fn(|index| {
        Pos2::new(
            rect.left() + 8.,
            plot.top() + index as f32 * lane_height + 2.,
        )
    });
    let traces = std::array::from_fn(|index| {
        let top = plot.top() + index as f32 * lane_height;
        Rect::from_min_max(
            Pos2::new(plot.left(), top + label_height + 4.),
            Pos2::new(plot.right(), top + lane_height - 3.),
        )
    });
    WaveformLayout {
        plot,
        traces,
        labels,
    }
}
#[cfg(test)]
mod waveform_layout_tests {
    use super::*;
    #[test]
    fn compact_and_large_waveforms_reserve_label_and_tick_bands() {
        for height in [96., 120., 160.] {
            for label_height in [14., 16., 20.] {
                let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(450., height));
                let layout = waveform_layout(rect, label_height);
                for index in 0..2 {
                    let label =
                        Rect::from_min_size(layout.labels[index], egui::vec2(150., label_height));
                    assert!(layout.traces[index].is_positive());
                    assert!(label.bottom() + 1. <= layout.traces[index].top());
                    if index == 0 {
                        assert!(layout.traces[index].bottom() < layout.labels[1].y);
                    }
                    assert!(layout.traces[index].bottom() < rect.bottom() - label_height - 5.);
                }
            }
        }
    }
}
