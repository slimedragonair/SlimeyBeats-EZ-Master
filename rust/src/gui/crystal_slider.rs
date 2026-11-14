//! Native Slider input and value editing, with its painted handle replaced by a jewel.
use super::theme;
use eframe::egui::{self, Color32, Pos2, Response, Shape, Stroke, Widget, WidgetText};
use std::ops::RangeInclusive;

#[must_use = "Add this slider to a Ui"]
pub struct CrystalSlider<'a> {
    inner: egui::Slider<'a>,
}

impl<'a> CrystalSlider<'a> {
    pub fn new<Num: egui::emath::Numeric>(
        value: &'a mut Num,
        range: impl Into<RangeInclusive<Num>>,
    ) -> Self {
        Self {
            // Native Always clamping also rounds an existing value to display precision.
            // Revealing a control must not silently change a preset or MCP parameter.
            inner: egui::Slider::new(value, range).clamping(egui::SliderClamping::Edits),
        }
    }

    pub fn text(mut self, text: impl Into<WidgetText>) -> Self {
        self.inner = self.inner.text(text);
        self
    }

    pub fn suffix(mut self, suffix: impl egui::IntoAtoms<'a>) -> Self {
        self.inner = self.inner.suffix(suffix);
        self
    }

    pub fn fixed_decimals(mut self, decimals: usize) -> Self {
        self.inner = self.inner.fixed_decimals(decimals);
        self
    }

    pub fn show_value(mut self, show: bool) -> Self {
        self.inner = self.inner.show_value(show);
        self
    }
}

impl Widget for CrystalSlider<'_> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        ui.scope(|ui| {
            ui.visuals_mut().selection.bg_fill = Color32::from_rgb(167, 69, 226);
            ui.visuals_mut().widgets.inactive.bg_fill = Color32::from_rgb(43, 32, 55);
            let layer = ui.painter().layer_id();
            let start = ui
                .ctx()
                .graphics(|g| g.get(layer).map_or(0, |list| list.next_idx().0));
            let response = ui.add(
                self.inner
                    .handle_shape(egui::style::HandleShape::Circle)
                    .trailing_fill(true),
            );
            // egui 0.37 slider_core::paint_handle emits one Circle at the actual mapped
            // value, including interaction expansion. Use that shape, never response.rect:
            // the latter also includes the editable value and optional label.
            let handle = ui.ctx().graphics(|g| {
                g.get(layer).and_then(|list| {
                    list.all_entries()
                        .enumerate()
                        .skip(start)
                        .find_map(|(index, clipped)| match &clipped.shape {
                            Shape::Circle(circle) => Some((
                                egui::layers::ShapeIdx(index),
                                clipped.clip_rect,
                                circle.center,
                                circle.radius,
                            )),
                            _ => None,
                        })
                })
            });
            if let Some((index, clip_rect, center, radius)) = handle {
                ui.painter().with_clip_rect(clip_rect).set(
                    index,
                    jewel(
                        center,
                        radius,
                        response.hovered() || response.dragged() || response.has_focus(),
                        ui.is_enabled(),
                    ),
                );
            }
            response
        })
        .inner
    }
}

fn jewel(center: Pos2, radius: f32, highlighted: bool, enabled: bool) -> Shape {
    let radius = radius * 0.94;
    let top = center - egui::vec2(0., radius);
    let right = center + egui::vec2(radius * 0.78, 0.);
    let bottom = center + egui::vec2(0., radius);
    let left = center - egui::vec2(radius * 0.78, 0.);
    let edge = if highlighted && enabled {
        Color32::WHITE
    } else {
        theme::AMETHYST
    };
    let mut shapes = Vec::with_capacity(8);
    if highlighted && enabled {
        shapes.push(Shape::convex_polygon(
            vec![top, right, bottom, left],
            Color32::TRANSPARENT,
            Stroke::new(5., Color32::from_rgba_unmultiplied(174, 75, 237, 25)),
        ));
    }
    shapes.push(Shape::convex_polygon(
        vec![top, right, bottom, left],
        Color32::from_rgb(54, 21, 76),
        Stroke::new(1.1, edge),
    ));
    let inset = radius * 0.19;
    let inner_top = top + egui::vec2(0., inset);
    let inner_right = right - egui::vec2(inset, 0.);
    let inner_bottom = bottom - egui::vec2(0., inset);
    let inner_left = left + egui::vec2(inset, 0.);
    for (points, color) in [
        (
            vec![inner_top, inner_right, center],
            Color32::from_rgb(184, 100, 233),
        ),
        (
            vec![inner_right, inner_bottom, center],
            Color32::from_rgb(90, 34, 135),
        ),
        (
            vec![inner_bottom, inner_left, center],
            Color32::from_rgb(132, 58, 192),
        ),
        (
            vec![inner_left, inner_top, center],
            Color32::from_rgb(236, 191, 255),
        ),
    ] {
        shapes.push(Shape::convex_polygon(points, color, Stroke::NONE));
    }
    shapes.push(Shape::line(
        vec![left, top, right],
        Stroke::new(0.8, Color32::from_rgb(244, 217, 255)),
    ));
    Shape::Vec(shapes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        value: &mut f64,
        skinned: bool,
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
                let result = if skinned {
                    ui.add_enabled(
                        enabled,
                        CrystalSlider::new(value, -6.0..=0.0)
                            .fixed_decimals(2)
                            .show_value(false),
                    )
                } else {
                    ui.add_enabled(
                        enabled,
                        egui::Slider::new(value, -6.0..=0.0)
                            .handle_shape(egui::style::HandleShape::Circle)
                            .fixed_decimals(2)
                            .show_value(false),
                    )
                };
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
    fn pointer_and_keyboard_values_match_native_slider() {
        let contexts = [egui::Context::default(), egui::Context::default()];
        let mut values = [-3., -3.];
        let mut positions = [Pos2::ZERO; 2];
        for i in 0..2 {
            let response = frame(&contexts[i], vec![], &mut values[i], i == 1, true, true);
            positions[i] = egui::pos2(
                response.rect.left() + response.rect.width() * 0.78,
                response.rect.center().y,
            );
        }
        for pressed in [true, false] {
            for i in 0..2 {
                let pos = positions[i];
                let events = vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ];
                frame(&contexts[i], events, &mut values[i], i == 1, true, true);
            }
            assert_eq!(values[0], values[1]);
        }
        let before = values[0];
        for i in 0..2 {
            let events = vec![egui::Event::Key {
                key: egui::Key::ArrowLeft,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }];
            assert!(frame(&contexts[i], events, &mut values[i], i == 1, true, false).changed());
        }
        assert_eq!(values[0], values[1]);
        assert!(values[0] < before);
    }

    #[test]
    fn disabled_slider_does_not_accept_pointer_input() {
        let ctx = egui::Context::default();
        let mut value = -3.;
        let rect = frame(&ctx, vec![], &mut value, true, false, false).rect;
        let pos = rect.right_center();
        let response = frame(
            &ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            &mut value,
            true,
            false,
            false,
        );
        assert!(!response.enabled());
        assert!(!response.changed());
        assert_eq!(value, -3.);
    }

    #[test]
    fn value_editors_support_f32_and_f64_without_changing_precision() {
        let ctx = egui::Context::default();
        let mut f32_value = 0.37_f32;
        let mut f64_value = -1.125_f64;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.add(
                CrystalSlider::new(&mut f32_value, 0.0..=1.0)
                    .text("Volume")
                    .show_value(false),
            );
            ui.add(CrystalSlider::new(&mut f64_value, -6.0..=0.0).suffix(" dBTP"));
        });
        output.textures_delta.clear();
        assert_eq!(f32_value, 0.37);
        assert_eq!(f64_value, -1.125);
    }

    #[test]
    fn jewel_center_matches_native_handle_with_editable_value_and_suffix() {
        for value in [-6., -3., -1., 0.] {
            let mut centers = Vec::new();
            for skinned in [false, true] {
                let ctx = egui::Context::default();
                let mut current = value;
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    if skinned {
                        ui.add(CrystalSlider::new(&mut current, -6.0..=0.0).suffix(" dBTP"));
                    } else {
                        ui.add(
                            egui::Slider::new(&mut current, -6.0..=0.0)
                                .handle_shape(egui::style::HandleShape::Circle)
                                .suffix(" dBTP"),
                        );
                    }
                });
                centers.push(
                    output
                        .shapes
                        .iter()
                        .find_map(|clipped| match &clipped.shape {
                            Shape::Circle(circle) => Some(circle.center),
                            Shape::Vec(parts) => parts.iter().find_map(|shape| match shape {
                                Shape::Path(path) if path.points.len() == 4 => {
                                    Some(path.visual_bounding_rect().center())
                                }
                                _ => None,
                            }),
                            _ => None,
                        }),
                );
                output.textures_delta.clear();
            }
            assert!(centers[0].is_some());
            assert_eq!(centers[0], centers[1], "handle mapping at {value}");
        }
    }
}
