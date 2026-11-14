use super::{app::MasterCraftApp, plots, theme, welcome_panel};
use eframe::egui::{self, RichText};

pub fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    let result = app.current().map(|(_, r)| r);
    measurement_heading(
        ui,
        if app.eq_dirty && result.as_ref().is_some_and(|r| r.report.is_some()) {
            "MASTER MEASUREMENTS · PREVIOUS REVISION"
        } else if result.as_ref().is_some_and(|r| r.report.is_some()) {
            "MASTER MEASUREMENTS"
        } else {
            "INPUT MEASUREMENTS"
        },
    );
    if let Some(r) = &result {
        let before = &r.analysis.metrics;
        let after = r
            .report
            .as_ref()
            .map(|v| &v.output.metrics)
            .unwrap_or(before);
        let columns = 3;
        let values = [
            (
                "LOUDNESS",
                format!("{:.1}", after.integrated_lufs),
                "LUFS",
                format!("Original {:.1}", before.integrated_lufs),
            ),
            (
                "TRUE PEAK",
                format!("{:.2}", after.true_peak_dbtp),
                "dBTP",
                format!("Original {:.2}", before.true_peak_dbtp),
            ),
            (
                "DYNAMICS",
                format!("{:.1}", after.lra_lu),
                "LU range",
                format!("Crest {:.1} dB", after.crest_db),
            ),
        ];
        for chunk in values.chunks(columns) {
            ui.columns(columns, |cols| {
                for (col, v) in cols.iter_mut().zip(chunk) {
                    egui::Frame::new()
                        .fill(theme::SURFACE)
                        .stroke(egui::Stroke::new(1., theme::BORDER))
                        .inner_margin(10)
                        .corner_radius(4)
                        .show(col, |ui| {
                            ui.spacing_mut().item_spacing.y = 4.;
                            theme::eyebrow(ui, v.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(&v.1).size(27.).color(theme::SLIME));
                                ui.label(RichText::new(v.2).size(14.).color(theme::MUTED));
                            });
                            ui.small(&v.3);
                        });
                }
            });
        }
        ui.add_space(10.);
        if ui.ctx().viewport_rect().height() < 800. {
            let pending_scroll = ui.make_persistent_id("overview-frequency-reveal");
            let chart = egui::CollapsingHeader::new("Frequency balance · spectrum")
                .id_salt("overview-frequency-balance")
                .show(ui, |ui| frequency_chart(ui, r));
            if chart.header_response.clicked() {
                let opening = egui::collapsing_header::CollapsingState::load(
                    ui.ctx(),
                    chart.header_response.id,
                )
                .is_some_and(|state| state.is_open());
                ui.data_mut(|data| data.insert_temp(pending_scroll, opening));
            }
            // Reveal the graph after expansion, rather than leaving its heading
            // under the pinned player and requiring another discovery step.
            if chart.openness >= 1.
                && ui
                    .data(|data| data.get_temp::<bool>(pending_scroll))
                    .unwrap_or(false)
                && let Some(body) = chart.body_response
            {
                body.scroll_to_me(Some(egui::Align::Min));
                ui.data_mut(|data| data.remove::<bool>(pending_scroll));
            }
        } else {
            frequency_chart(ui, r);
        }
        if let Some(report) = &r.report {
            ui.small(format!(
                "Requested {:.1} LUFS · measured {:.1} LUFS",
                report.target_lufs, report.output.metrics.integrated_lufs
            ));
            if !report.limiter.reachable {
                ui.label(RichText::new("Peak or gain limits affected the target. Compare the preview and read the report before exporting.").color(theme::BONE));
            }
        } else {
            ui.label(
                RichText::new(format!(
                    "Suggested profile: {}",
                    r.analysis.suggested_profile.replace('_', " ")
                ))
                .size(14.)
                .color(theme::MUTED),
            );
        }
    } else {
        welcome_panel::show(app, ui);
    }
}

fn frequency_chart(ui: &mut egui::Ui, result: &super::worker::TrackResult) {
    ui.horizontal_wrapped(|ui| {
        theme::eyebrow(ui, "FREQUENCY BALANCE");
        legend(ui, "Original", theme::MUTED);
        if result.report.is_some() {
            legend(ui, "Master", theme::SLIME);
        }
    });
    ui.label(
        RichText::new("Level / dBFS per bin   ·   Frequency / Hz")
            .size(13.)
            .color(theme::MUTED),
    );
    plots::spectrum(
        ui,
        Some(&result.analysis.spectrum),
        result.report.as_ref().map(|report| &report.output.spectrum),
        if ui.ctx().viewport_rect().height() < 800. {
            140.
        } else {
            170.
        },
    );
}

fn measurement_heading(ui: &mut egui::Ui, label: &str) {
    ui.horizontal(|ui| {
        theme::eyebrow(ui, label);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(0.), 14.),
            egui::Sense::hover(),
        );
        let start = rect.left_center();
        ui.painter().line_segment(
            [start, rect.right_center()],
            egui::Stroke::new(1., theme::BORDER),
        );
        if rect.width() > 8. {
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    start + egui::vec2(0., -3.),
                    start + egui::vec2(4., 0.),
                    start + egui::vec2(0., 3.),
                    start + egui::vec2(-4., 0.),
                ],
                theme::AMETHYST,
                egui::Stroke::NONE,
            ));
        }
    });
    ui.add_space(5.);
}

fn legend(ui: &mut egui::Ui, label: &str, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18., 12.), egui::Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        egui::Stroke::new(2., color),
    );
    ui.label(RichText::new(label).size(14.).color(color));
}
pub fn report(app: &MasterCraftApp, ui: &mut egui::Ui) {
    let Some((_, r)) = app.current() else {
        ui.label("Analyze or master a track to inspect its report.");
        return;
    };
    if app.eq_dirty {
        ui.label(RichText::new("Live EQ uses an estimate. These measurements describe the previous revision while full-track measurements prepare.").color(theme::BONE));
    }
    theme::eyebrow(ui, "INPUT FINDINGS");
    ui.label(RichText::new("These automated flags suggest things to listen for. A bigger number means a stronger detected pattern, not a quality grade.").size(14.).color(theme::MUTED));
    for issue in &r.analysis.issues {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!("{:>3.0} / 100", issue.severity))
                    .monospace()
                    .color(if issue.severity >= 30. {
                        theme::BONE
                    } else {
                        theme::MUTED
                    }),
            );
            ui.label(RichText::new(&issue.key).strong());
            ui.label(&issue.detail);
        });
    }
    if let Some(report) = &r.report {
        ui.add_space(12.);
        theme::eyebrow(ui, "PROCESSING CHAIN");
        egui::Grid::new("stages")
            .num_columns(2)
            .striped(true)
            .spacing([28., 8.])
            .show(ui, |ui| {
                for stage in &report.log {
                    ui.label(&stage.module).on_hover_text(
                        serde_json::to_string_pretty(&stage.detail).unwrap_or_default(),
                    );
                    ui.monospace(format!("{:.3} s", stage.seconds));
                    ui.end_row();
                }
            });
        ui.add_space(12.);
        theme::eyebrow(ui, "VERIFICATION & NOTES");
        ui.label(format!("Target {:.1} LUFS · Ceiling {:.1} dBTP · Limiter reduction {:.1} dB · Punch loss {:.1} dB",report.target_lufs,report.ceiling_dbtp,report.limiter.max_gr_db,report.punch_loss_db));
        for note in &report.notes {
            ui.label(note);
        }
        for export in &report.exports {
            ui.separator();
            ui.label(RichText::new(export["path"].as_str().unwrap_or("")).color(theme::SLIME));
            ui.small(format!(
                "Decoded export: {:.2} LUFS / {:.2} dBTP · {} Hz",
                export["decoded_metrics"]["integrated_lufs"]
                    .as_f64()
                    .unwrap_or(0.),
                export["decoded_metrics"]["true_peak_dbtp"]
                    .as_f64()
                    .unwrap_or(0.),
                export["sample_rate"]
            ));
        }
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::gui::{test_fixture, ui_test_input::*};

    #[test]
    fn compact_spectrum_is_an_explicit_disclosure_and_playback_stays_visible() {
        for advanced in [false, true] {
            let ctx = egui::Context::default();
            theme::apply(&ctx);
            ctx.all_styles_mut(|style| style.animation_time = 0.);
            let viewport = egui::vec2(1100., 720.);
            let mut app = test_fixture::mastered(&ctx);
            app.tab = 0;
            app.experience.preferences.advanced = advanced;
            let output = settled(&ctx, &mut app, viewport);
            let settings = serde_json::to_value(&app.settings).unwrap();
            for label in ["Play", "Stop", "A · Original", "B · Master"] {
                assert!(
                    text_rect(&output, label).is_some(),
                    "Playback hidden: {label}"
                );
            }
            let tab_tops: Vec<_> = ["Overview", "Manual EQ", "Studio"]
                .iter()
                .map(|label| text_rect(&output, label).unwrap().top())
                .collect();
            let top = tab_tops.iter().copied().fold(f32::INFINITY, f32::min);
            let bottom = tab_tops.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            assert!(bottom - top < 4., "Workspace tabs wrapped: {tab_tops:?}");
            assert!(text_rect(&output, "FREQUENCY BALANCE").is_none());
            assert!(
                text_rect(&output, "Frequency balance · spectrum").is_some(),
                "Spectrum disclosure clipped: {:?}",
                output
                    .shapes
                    .iter()
                    .filter_map(|item| match &item.shape {
                        egui::Shape::Text(text)
                            if text.galley.text().contains("Frequency balance") =>
                            Some((text.visual_bounding_rect(), item.clip_rect)),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            );
            let output = click(
                &ctx,
                &mut app,
                viewport,
                &output,
                "Frequency balance · spectrum",
            );
            assert!(
                text_rect(&output, "FREQUENCY BALANCE").is_some(),
                "Opened spectrum heading should be inside the scrolling viewport"
            );
            assert!(text_rect(&output, "Level / dBFS per bin   ·   Frequency / Hz").is_some());
            assert_eq!(serde_json::to_value(&app.settings).unwrap(), settings);
            assert!(text_rect(&output, "Play").is_some());
        }
    }
}
