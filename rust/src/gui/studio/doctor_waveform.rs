//! Overlay measured regions on the existing, worker-prepared waveform envelope.
use super::{doctor_controls::issue_color, state::Studio};
use eframe::egui::{self, Pos2, Rect, Stroke};

pub(super) fn show(studio: &mut Studio, ui: &mut egui::Ui) {
    let Some(findings) = &studio.doctor.findings else {
        return;
    };
    let duration = findings.report.duration_seconds;
    let graph = ui.scope(|ui| {
        findings.waveform.show_labeled(
            ui,
            studio.player.seconds(),
            duration,
            ("SCANNED INPUT", "ISSUE RANGES"),
            112.,
        )
    });
    let rect = graph.response.rect;
    // Waveform::show_labeled uses a 12-point horizontal inset for time coordinates.
    let plot = Rect::from_min_max(
        rect.min + egui::vec2(12., 0.),
        rect.max - egui::vec2(12., 22.),
    );
    let painter = ui.painter().with_clip_rect(plot.intersect(ui.clip_rect()));
    let x = |seconds: f64| {
        plot.left() + (seconds / duration.max(0.001)).clamp(0., 1.) as f32 * plot.width()
    };
    let mut clicked = None;
    for (index, issue) in findings.report.issues.iter().enumerate() {
        let color = issue_color(issue.kind);
        let region = Rect::from_min_max(
            Pos2::new(x(issue.start_seconds), plot.top() + 20.),
            Pos2::new(
                x(issue.end_seconds).max(x(issue.start_seconds) + 2.),
                plot.bottom(),
            ),
        );
        painter.rect_filled(
            region,
            0.,
            color.gamma_multiply(if studio.doctor.selected == Some(index) {
                0.28
            } else {
                0.12
            }),
        );
        painter.line_segment(
            [region.left_top(), region.left_bottom()],
            Stroke::new(
                if studio.doctor.selected == Some(index) {
                    2.
                } else {
                    1.
                },
                color,
            ),
        );
        let marker = Rect::from_min_max(
            Pos2::new(region.left(), plot.bottom() - 14.),
            region.right_bottom(),
        );
        painter.rect_filled(marker, 0., color.gamma_multiply(0.7));
        if ui
            .interact(
                marker.expand2(egui::vec2(2., 0.)),
                ui.make_persistent_id(("doctor-marker", index)),
                egui::Sense::click(),
            )
            .on_hover_text(format!(
                "{} · {}",
                issue.title,
                super::doctor_time::range_label(
                    issue.start_seconds,
                    issue.end_seconds,
                    findings.report.sample_rate
                )
            ))
            .clicked()
        {
            clicked = Some(index);
        }
    }
    if let Some(seconds) = graph.inner {
        studio.player.seek(seconds);
    }
    if let Some(index) = clicked {
        studio.select_doctor_issue(index);
    }
}
