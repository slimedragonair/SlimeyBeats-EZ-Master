//! The unmeasured workspace: portrait, getting-started steps, and local-processing help.
use super::{app::MasterCraftApp, crystal_button, theme};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke};

const STEP_FILL: Color32 = Color32::from_rgb(34, 25, 46);
const STEP_EDGE: Color32 = Color32::from_rgb(117, 85, 143);
const STEP_INK: Color32 = Color32::from_rgb(237, 224, 250);
const BADGE_SIZE: f32 = 46.;
const ICON_SIZE: f32 = 30.;
const STEP_GAP: f32 = 12.;
const HERO_GAP: f32 = 18.;
const HERO_COPY_GAP: f32 = 9.;

pub fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    ui.add_space(5.);
    hero(app, ui);
    if app.tracks.is_empty() && app.example_path().is_some() {
        ui.add_space(8.);
        if ui.add(crystal_button::primary("Try the example track")).on_hover_text("Add the bundled example to the mastering queue. You choose when to analyze, master and export.").clicked() { app.add_example(); }
    }
    ui.add_space(10.);
    for (number, title, body, icon) in [
        (
            "01",
            "Add your tracks",
            "Drop audio onto the window or use Add audio files.",
            StepIcon::Wave,
        ),
        (
            "02",
            "Choose the direction",
            "Pick a tonal profile or reference track and set your loudness target.",
            StepIcon::Settings,
        ),
        (
            "03",
            "Master, compare, export",
            "Inspect the result, compare at matched loudness, and fine-tune with the seven-band EQ.",
            StepIcon::Layers,
        ),
    ] {
        step_card(ui, number, title, body, icon);
    }
    ui.add_space(5.);
    processing_note(ui);
}

fn hero(app: &MasterCraftApp, ui: &mut egui::Ui) {
    let width = ui.available_width();
    let portrait_size = (width * 0.30).clamp(128., 220.);
    let text_width = (width - portrait_size - HERO_GAP).max(60.);
    let copy = [
        theme::display(
            if app.tracks.is_empty() {
                "Shape your sound."
            } else {
                "Your mix is loaded."
            },
            if width < 560. { 28. } else { 36. },
        ),
        RichText::new(if app.tracks.is_empty() {
            "Add a mix or try the example. Master creates a preview; Export saves it."
        } else {
            "Choose Analyze to understand the mix, or Master to prepare a preview."
        })
        .size(17.)
        .color(STEP_INK),
        RichText::new("Help explains the controls and can replay the quick tour.")
            .size(14.)
            .color(theme::AMETHYST),
    ]
    .map(|text| {
        egui::WidgetText::from(text).into_galley(
            ui,
            Some(egui::TextWrapMode::Wrap),
            text_width,
            egui::TextStyle::Body,
        )
    });
    let copy_height = copy.iter().map(|text| text.size().y).sum::<f32>() + HERO_COPY_GAP * 2.;
    let hero_height = portrait_size.max(copy_height);
    let copy_top_gap = (hero_height - copy_height) * 0.5;
    let hero_rect = Rect::from_min_size(ui.next_widget_position(), egui::vec2(width, hero_height));
    // Generated atmosphere stays behind the welcome content and has no input region.
    app.brand.show_ribbons(ui, hero_rect, 0.42);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = HERO_GAP;
        app.brand.show(ui, portrait_size);
        // Measure wrapping first: a zero-height centered child starts at the portrait's midpoint.
        ui.allocate_ui_with_layout(
            egui::vec2(text_width, hero_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(text_width);
                ui.spacing_mut().item_spacing.y = HERO_COPY_GAP;
                ui.add_space(copy_top_gap);
                for text in copy {
                    ui.add(egui::Label::new(text));
                }
            },
        );
    });
}

#[derive(Clone, Copy)]
enum StepIcon {
    Wave,
    Settings,
    Layers,
}

fn step_card(ui: &mut egui::Ui, number: &str, title: &str, body: &str, icon: StepIcon) {
    // Reserve a shape before laying out text so the fitted frame paints underneath it.
    let background = ui.painter().add(egui::Shape::Noop);
    let frame = egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = STEP_GAP;
                number_badge(ui, number);
                let text_width = (ui.available_width() - ICON_SIZE - STEP_GAP).max(60.);
                ui.allocate_ui_with_layout(
                    egui::vec2(text_width, 0.),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(text_width);
                        ui.spacing_mut().item_spacing.y = 2.;
                        ui.add(egui::Label::new(theme::display(title, 21.).color(STEP_INK)).wrap());
                        ui.add(
                            egui::Label::new(RichText::new(body).size(15.).color(STEP_INK)).wrap(),
                        );
                    },
                );
                let (slot, _) =
                    ui.allocate_exact_size(egui::vec2(ICON_SIZE, BADGE_SIZE), Sense::hover());
                let rect = Rect::from_center_size(slot.center(), egui::Vec2::splat(ICON_SIZE));
                paint_step_icon(ui, rect, icon);
            });
        });
    let rect = frame.response.rect;
    let cut = 9.;
    ui.painter().set(
        background,
        egui::Shape::convex_polygon(
            vec![
                rect.left_top() + egui::vec2(cut, 0.),
                rect.right_top() - egui::vec2(cut, 0.),
                rect.right_top() + egui::vec2(0., cut),
                rect.right_bottom() - egui::vec2(0., cut),
                rect.right_bottom() - egui::vec2(cut, 0.),
                rect.left_bottom() + egui::vec2(cut, 0.),
                rect.left_bottom() - egui::vec2(0., cut),
                rect.left_top() + egui::vec2(0., cut),
            ],
            STEP_FILL,
            Stroke::new(1., STEP_EDGE),
        ),
    );
    ui.painter().line_segment(
        [
            rect.left_top() + egui::vec2(cut + 1., 1.),
            rect.right_top() - egui::vec2(cut + 1., -1.),
        ],
        Stroke::new(0.7, theme::AMETHYST.gamma_multiply(0.55)),
    );
}

fn number_badge(ui: &mut egui::Ui, number: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(BADGE_SIZE), Sense::hover());
    let diamond = rect.shrink(1.);
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            diamond.center_top(),
            diamond.right_center(),
            diamond.center_bottom(),
            diamond.left_center(),
        ],
        Color32::from_rgb(49, 29, 66),
        Stroke::new(1.2, theme::AMETHYST),
    ));
    ui.place(
        rect,
        egui::Label::new(theme::display(number, 21.).color(STEP_INK)),
    );
}

fn paint_step_icon(ui: &egui::Ui, rect: Rect, icon: StepIcon) {
    let painter = ui.painter();
    let center = rect.center();
    let stroke = Stroke::new(1.6, theme::AMETHYST);
    match icon {
        StepIcon::Wave => {
            // A small instructional icon, never a stand-in for measured audio.
            let points = (0..=32)
                .map(|i| {
                    let t = i as f32 / 32.;
                    let taper = (std::f32::consts::PI * t).sin();
                    Pos2::new(
                        rect.left() + t * rect.width(),
                        center.y - (t * std::f32::consts::TAU * 2.).sin() * taper * 10.,
                    )
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
        }
        StepIcon::Settings => {
            for (x, y) in [(-10., -5.), (0., 6.), (10., -1.)] {
                let node = center + egui::vec2(x, y);
                painter.line_segment(
                    [center + egui::vec2(x, -14.), node - egui::vec2(0., 3.)],
                    stroke,
                );
                painter.line_segment(
                    [node + egui::vec2(0., 3.), center + egui::vec2(x, 14.)],
                    stroke,
                );
                painter.circle_stroke(node, 3., stroke);
            }
        }
        StepIcon::Layers => {
            for y in [-8., 0., 8.] {
                let c = center + egui::vec2(0., y);
                painter.add(egui::Shape::closed_line(
                    vec![
                        c - egui::vec2(14., 0.),
                        c - egui::vec2(0., 7.),
                        c + egui::vec2(14., 0.),
                        c + egui::vec2(0., 7.),
                    ],
                    stroke,
                ));
            }
        }
    }
}

fn processing_note(ui: &mut egui::Ui) {
    ui.separator();
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(20., 20.), Sense::hover());
        ui.painter()
            .circle_stroke(rect.center(), 8., Stroke::new(1., theme::AMETHYST));
        ui.place(rect, egui::Label::new(theme::display("i", 15.)));
        ui.add(
            egui::Label::new(RichText::new("Processing stays on your computer. WAV mastering works without Python or FFmpeg. Compressed formats and resampling use FFmpeg.").size(14.).color(theme::MUTED)).wrap(),
        );
    });
}
