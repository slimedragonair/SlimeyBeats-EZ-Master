//! Clipped glass paint over egui's native button layout, focus, and accessibility.
use super::{
    skin_icons::{self, Icon},
    theme,
};
use eframe::egui::{
    self, Color32, Mesh, Pos2, Rect, Response, Shape, Stroke, Vec2, Widget, WidgetText,
};

#[must_use = "Add this button to a Ui"]
pub struct CrystalButton {
    text: WidgetText,
    primary: bool,
    selected: Option<bool>,
    min_size: Vec2,
    wrap: bool,
    icon: Option<Icon>,
}

pub fn primary(text: impl Into<WidgetText>) -> CrystalButton {
    CrystalButton::new(text, true)
}

pub fn secondary(text: impl Into<WidgetText>) -> CrystalButton {
    CrystalButton::new(text, false)
}

impl CrystalButton {
    fn new(text: impl Into<WidgetText>, primary: bool) -> Self {
        Self {
            text: text.into(),
            primary,
            selected: None,
            min_size: Vec2::ZERO,
            wrap: false,
            icon: None,
        }
    }

    pub fn min_size(mut self, size: Vec2) -> Self {
        self.min_size = size;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }

    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

impl Widget for CrystalButton {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let background = ui.painter().add(Shape::Noop);
        let text = self
            .text
            .color(Color32::WHITE)
            .fallback_text_style(egui::TextStyle::Button);
        let mut button = if let Some(icon) = self.icon {
            let icon = egui::Atom::paint(Vec2::splat(17.), move |ui, args| {
                skin_icons::paint(ui, args.rect, icon, Color32::WHITE);
            });
            egui::Button::new((icon, text))
        } else {
            egui::Button::new(text)
        };
        button = button
            .min_size(self.min_size)
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE);
        if let Some(selected) = self.selected {
            button = button.selected(selected);
        }
        if self.wrap {
            button = button.wrap();
        }
        let response = ui.add(button);
        if ui.is_rect_visible(response.rect) {
            ui.painter().set(
                background,
                glass_frame(
                    response.rect.shrink(0.75),
                    self.primary || self.selected == Some(true),
                    response.hovered(),
                    response.is_pointer_button_down_on(),
                    response.has_focus(),
                    ui.is_enabled(),
                ),
            );
        }
        response
    }
}

fn glass_frame(
    rect: Rect,
    active: bool,
    hovered: bool,
    pressed: bool,
    focused: bool,
    enabled: bool,
) -> Shape {
    let lit = active && enabled;
    let cut = (rect.height() * 0.2).clamp(4., 8.);
    let outline = clipped_corners(rect, cut);
    let (top, middle, bottom) = if !enabled {
        (
            Color32::from_rgb(48, 42, 55),
            Color32::from_rgb(29, 25, 35),
            Color32::from_rgb(37, 30, 44),
        )
    } else if pressed {
        (
            Color32::from_rgb(53, 23, 75),
            Color32::from_rgb(61, 24, 87),
            Color32::from_rgb(110, 49, 154),
        )
    } else if lit {
        (
            Color32::from_rgb(134, 57, 181),
            Color32::from_rgb(61, 24, 93),
            Color32::from_rgb(112, 43, 167),
        )
    } else if hovered {
        (
            Color32::from_rgb(76, 56, 90),
            Color32::from_rgb(33, 25, 45),
            Color32::from_rgb(46, 31, 62),
        )
    } else {
        (
            Color32::from_rgb(61, 54, 70),
            Color32::from_rgb(29, 25, 36),
            Color32::from_rgb(38, 31, 48),
        )
    };
    let edge = if !enabled {
        theme::BORDER
    } else if lit || hovered || focused {
        theme::AMETHYST
    } else {
        theme::SILVER
    };
    let mut shapes = Vec::with_capacity(9);
    if lit || focused {
        for (width, alpha) in [(7., 10), (4., 23)] {
            shapes.push(Shape::closed_line(
                outline.to_vec(),
                Stroke::new(width, Color32::from_rgba_unmultiplied(184, 85, 252, alpha)),
            ));
        }
    }
    let mut mesh = Mesh::default();
    // Triangulate the clipped polygon around its center: the upper edge catches light,
    // while the center stays quiet beneath real, editable UI text.
    mesh.colored_vertex(rect.center(), middle);
    for point in outline {
        let t = ((point.y - rect.top()) / rect.height()).clamp(0., 1.);
        let color = if t < 0.5 {
            top.lerp_to_gamma(middle, t * 2.)
        } else {
            middle.lerp_to_gamma(bottom, (t - 0.5) * 2.)
        };
        mesh.colored_vertex(point, color);
    }
    for i in 0..8 {
        mesh.add_triangle(0, i + 1, (i + 1) % 8 + 1);
    }
    shapes.push(Shape::mesh(mesh));
    shapes.push(Shape::closed_line(
        outline.to_vec(),
        Stroke::new(if lit { 1.4 } else { 1. }, edge),
    ));
    let inner = clipped_corners(rect.shrink(2.), (cut - 1.).max(2.));
    shapes.push(Shape::line(
        inner[..4].to_vec(),
        Stroke::new(
            1.,
            if lit {
                Color32::from_rgb(242, 206, 255)
            } else {
                Color32::from_rgb(142, 128, 155)
            },
        ),
    ));
    shapes.push(Shape::line(
        inner[4..].to_vec(),
        Stroke::new(
            1.,
            if lit {
                Color32::from_rgb(161, 74, 218)
            } else {
                Color32::from_rgb(48, 38, 58)
            },
        ),
    ));
    if focused && enabled {
        shapes.push(Shape::closed_line(
            clipped_corners(rect.shrink(3.5), (cut - 2.).max(2.)).to_vec(),
            Stroke::new(1., Color32::WHITE),
        ));
    }
    Shape::Vec(shapes)
}

fn clipped_corners(rect: Rect, cut: f32) -> [Pos2; 8] {
    let cut = cut.min(rect.width() * 0.25).min(rect.height() * 0.5);
    [
        egui::pos2(rect.left(), rect.top() + cut),
        egui::pos2(rect.left() + cut, rect.top()),
        egui::pos2(rect.right() - cut, rect.top()),
        egui::pos2(rect.right(), rect.top() + cut),
        egui::pos2(rect.right(), rect.bottom() - cut),
        egui::pos2(rect.right() - cut, rect.bottom()),
        egui::pos2(rect.left() + cut, rect.bottom()),
        egui::pos2(rect.left(), rect.bottom() - cut),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        enabled: bool,
        focus: bool,
    ) -> Response {
        let mut response = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                let result = ui.add_enabled(
                    enabled,
                    secondary("Master queue")
                        .min_size(egui::vec2(180., 36.))
                        .selected(true)
                        .wrap()
                        .icon(Icon::Layers),
                );
                if focus {
                    result.request_focus();
                }
                response = Some(result);
            },
        );
        output.textures_delta.clear();
        response.unwrap()
    }

    #[test]
    fn native_keyboard_activation_and_disabled_state() {
        let ctx = egui::Context::default();
        let first = frame(&ctx, vec![], true, true);
        assert!(first.rect.width() >= 180. && first.rect.height() >= 36.);
        let enter = || egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(frame(&ctx, vec![enter()], true, false).clicked());
        let disabled = frame(&ctx, vec![enter()], false, false);
        assert!(!disabled.enabled());
        assert!(!disabled.clicked());
    }

    #[test]
    fn native_pointer_click_survives_custom_paint() {
        let ctx = egui::Context::default();
        let center = frame(&ctx, vec![], true, false).rect.center();
        let pointer = |pressed| egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &ctx,
            vec![egui::Event::PointerMoved(center), pointer(true)],
            true,
            false,
        );
        assert!(frame(&ctx, vec![pointer(false)], true, false).clicked());
    }
}
