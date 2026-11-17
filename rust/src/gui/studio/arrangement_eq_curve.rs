//! Transfer curves for the selected scope; this is not a loudness or source-spectrum meter.
use super::arrangement_state::ArrangementEditor;
use crate::{config::EqBand, dsp::biquad, gui::theme};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};

fn curve(bands: &[EqBand], gain: f64, rate: u32, trace: Rect, upper: f64) -> Vec<Pos2> {
    let filters: Vec<_> = bands
        .iter()
        .filter(|band| band.on)
        .map(|band| biquad::from_band(band, rate))
        .collect();
    (0..=160)
        .map(|index| {
            let fraction = index as f64 / 160.;
            let frequency = 20. * (upper / 20.).powf(fraction);
            let db = (gain
                + filters
                    .iter()
                    .map(|filter| filter.response_db(frequency, rate))
                    .sum::<f64>())
            .clamp(-18., 18.);
            Pos2::new(
                trace.left() + trace.width() * fraction as f32,
                trace.center().y - trace.height() * db as f32 / 36.,
            )
        })
        .collect()
}

pub(super) fn show(editor: &ArrangementEditor, ui: &mut egui::Ui, rate: u32) {
    ui.small("EQ response · gain / dB");
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(theme::SLIME, "Current");
        if editor.eq_proposal.is_some() {
            ui.colored_label(theme::BONE, "Suggested · not applied");
        }
    });
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(80.), 140.),
        Sense::hover(),
    );
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 5., Color32::from_rgb(19, 15, 27));
    let trace = Rect::from_min_max(
        rect.min + egui::vec2(25., 8.),
        rect.max - egui::vec2(5., 23.),
    );
    let upper = 20_000_f64.min(rate as f64 * 0.49);
    for db in [-12., 0., 12.] {
        let y = trace.center().y - trace.height() * db / 36.;
        painter.line_segment(
            [Pos2::new(trace.left(), y), Pos2::new(trace.right(), y)],
            Stroke::new(0.7, theme::MUTED.gamma_multiply(0.35)),
        );
        painter.text(
            Pos2::new(trace.left() - 3., y),
            Align2::RIGHT_CENTER,
            if db == 0. {
                "0".into()
            } else {
                format!("{db:+.0}")
            },
            FontId::proportional(10.),
            theme::MUTED,
        );
    }
    for frequency in [100., 1000., 10_000.] {
        if frequency > upper {
            continue;
        }
        let fraction = (frequency / 20_f64).ln() / (upper / 20.).ln();
        let x = trace.left() + trace.width() * fraction as f32;
        painter.line_segment(
            [Pos2::new(x, trace.top()), Pos2::new(x, trace.bottom())],
            Stroke::new(0.5, theme::MUTED.gamma_multiply(0.25)),
        );
        painter.text(
            Pos2::new(x, rect.bottom() - 3.),
            Align2::CENTER_BOTTOM,
            if frequency == 100. {
                "100 Hz"
            } else if frequency == 1000. {
                "1 kHz"
            } else {
                "10 kHz"
            },
            FontId::proportional(11.),
            theme::MUTED,
        );
    }
    let current = if editor.eq_bypass {
        curve(&[], 0., rate, trace, upper)
    } else {
        curve(&editor.eq_bands, editor.eq_output_db, rate, trace, upper)
    };
    painter.add(egui::Shape::line(current, Stroke::new(2., theme::SLIME)));
    if let Some(proposal) = &editor.eq_proposal {
        painter.add(egui::Shape::line(
            curve(&proposal.bands, editor.eq_output_db, rate, trace, upper),
            Stroke::new(1.5, theme::BONE),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Settings,
        dsp::{
            eq_chain::{EqChain, EqParameters},
            eq_suggestion::EqSuggestion,
        },
    };

    #[test]
    fn proposed_curve_matches_shared_output_gain_even_when_current_eq_is_bypassed() {
        for gain_db in [-6., 6.] {
            let ctx = egui::Context::default();
            let mut editor = ArrangementEditor::default();
            editor.eq_output_db = gain_db;
            editor.eq_bypass = true;
            let bands = crate::config::default_eq();
            editor.eq_proposal = Some(EqSuggestion {
                bands: bands.clone(),
                notes: vec![],
                high_frequency: None,
            });
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(500., 300.))),
                    ..Default::default()
                },
                |ui| show(&editor, ui, 48_000),
            );
            output.textures_delta.clear();
            let proposal = output
                .shapes
                .iter()
                .find_map(|item| match &item.shape {
                    egui::Shape::Path(path)
                        if path.points.len() == 161 && path.stroke.width == 1.5 =>
                    {
                        Some(&path.points)
                    }
                    _ => None,
                })
                .expect("Rendered proposal line");
            let grid: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|item| match &item.shape {
                    egui::Shape::LineSegment { points, stroke } if stroke.width == 0.7 => {
                        Some(points[0].y)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(grid.len(), 3);
            let pixels_per_db = (grid[0] - grid[2]) / 24.;
            let mut chain = EqChain::new(EqParameters::new(
                &Settings {
                    manual_eq: bands,
                    eq_output_db: gain_db,
                    eq_hold_loudness: false,
                    ..Default::default()
                },
                48_000,
                0.,
            ));
            let (mut input_energy, mut output_energy) = (0., 0.);
            for frame in 0..4800 {
                let input = 0.01 * (std::f64::consts::TAU * 1000. * frame as f64 / 48_000.).sin();
                let wet = chain.process([input, 0.], 1)[0];
                input_energy += input * input;
                output_energy += wet * wet;
            }
            let measured_db = 10. * (output_energy / input_energy).log10();
            for point in proposal {
                let plotted_db = (grid[1] - point.y) / pixels_per_db;
                assert!(
                    (plotted_db as f64 - measured_db).abs() < 1e-4,
                    "Proposal {plotted_db} dB differs from shared processor {measured_db} dB"
                );
            }
        }
    }
}
