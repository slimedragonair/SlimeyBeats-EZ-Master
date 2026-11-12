//! Small functional line icons. Detailed ornament belongs to the shared artwork assets.
use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke};

#[derive(Clone, Copy)]
pub enum Icon {
    Add,
    Play,
    Pause,
    Stop,
    Layers,
    Export,
    Analyze,
    Home,
    Sliders,
    Report,
    Trash,
    Clear,
}

pub fn paint(ui: &egui::Ui, rect: Rect, icon: Icon, color: Color32) {
    let p = |x: f32, y: f32| -> Pos2 {
        egui::pos2(
            rect.left() + x * rect.width(),
            rect.top() + y * rect.height(),
        )
    };
    let stroke = Stroke::new(1.5, color);
    let line = |points: &[(f32, f32)]| {
        ui.painter().add(Shape::line(
            points.iter().map(|&(x, y)| p(x, y)).collect(),
            stroke,
        ));
    };
    match icon {
        Icon::Add => {
            line(&[(0.5, 0.08), (0.5, 0.92)]);
            line(&[(0.08, 0.5), (0.92, 0.5)]);
        }
        Icon::Play => {
            ui.painter().add(Shape::convex_polygon(
                vec![p(0.23, 0.08), p(0.9, 0.5), p(0.23, 0.92)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            for x in [0.2, 0.65] {
                ui.painter().rect_filled(
                    Rect::from_min_max(p(x, 0.1), p(x + 0.16, 0.9)),
                    0.,
                    color,
                );
            }
        }
        Icon::Stop => {
            ui.painter().rect_stroke(
                Rect::from_min_max(p(0.18, 0.18), p(0.82, 0.82)),
                0.,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Icon::Layers => {
            line(&[
                (0.04, 0.3),
                (0.5, 0.06),
                (0.96, 0.3),
                (0.5, 0.54),
                (0.04, 0.3),
            ]);
            for y in [0.5, 0.72] {
                line(&[(0.06, y), (0.5, y + 0.22), (0.94, y)]);
            }
        }
        Icon::Export => {
            line(&[(0.1, 0.52), (0.1, 0.9), (0.9, 0.9), (0.9, 0.52)]);
            line(&[(0.5, 0.7), (0.5, 0.06)]);
            line(&[(0.24, 0.32), (0.5, 0.06), (0.76, 0.32)]);
        }
        Icon::Analyze => {
            for (x, height) in [
                (0.08, 0.2),
                (0.29, 0.65),
                (0.5, 0.95),
                (0.71, 0.5),
                (0.92, 0.25),
            ] {
                line(&[(x, 0.5 - height / 2.), (x, 0.5 + height / 2.)]);
            }
        }
        Icon::Home => {
            line(&[(0.02, 0.45), (0.5, 0.04), (0.98, 0.45)]);
            line(&[
                (0.17, 0.37),
                (0.17, 0.92),
                (0.4, 0.92),
                (0.4, 0.61),
                (0.63, 0.61),
                (0.63, 0.92),
                (0.84, 0.92),
                (0.84, 0.37),
            ]);
        }
        Icon::Sliders => {
            for (x, y) in [(0.16, 0.3), (0.5, 0.7), (0.84, 0.42)] {
                line(&[(x, 0.06), (x, y - 0.1)]);
                line(&[(x, y + 0.1), (x, 0.94)]);
                ui.painter()
                    .circle_stroke(p(x, y), rect.width() * 0.09, stroke);
            }
        }
        Icon::Report => {
            line(&[
                (0.18, 0.05),
                (0.65, 0.05),
                (0.9, 0.3),
                (0.9, 0.95),
                (0.18, 0.95),
                (0.18, 0.05),
            ]);
            line(&[(0.65, 0.05), (0.65, 0.3), (0.9, 0.3)]);
            for y in [0.45, 0.62, 0.79] {
                line(&[(0.33, y), (0.74, y)]);
            }
        }
        Icon::Trash => {
            line(&[(0.1, 0.23), (0.9, 0.23)]);
            line(&[(0.35, 0.23), (0.35, 0.07), (0.65, 0.07), (0.65, 0.23)]);
            line(&[(0.21, 0.23), (0.27, 0.93), (0.73, 0.93), (0.79, 0.23)]);
            for x in [0.41, 0.59] {
                line(&[(x, 0.37), (x, 0.78)]);
            }
        }
        Icon::Clear => {
            line(&[(0.78, 0.06), (0.52, 0.48)]);
            line(&[
                (0.4, 0.42),
                (0.7, 0.61),
                (0.51, 0.95),
                (0.11, 0.7),
                (0.4, 0.42),
            ]);
            line(&[(0.24, 0.68), (0.48, 0.83)]);
        }
    }
}
