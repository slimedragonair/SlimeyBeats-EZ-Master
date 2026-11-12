//! A shared nine-slice transform keeps the generated metal connected at every corner.
use super::brand_art;
use eframe::egui::{self, Color32, Rect, Vec2, pos2};

const FRAME: &[u8] = include_bytes!("../../assets/sovereign-panel-frame.png");
const SOURCE_MARGIN: f32 = 128.;

#[derive(Clone)]
struct FrameTexture(egui::TextureHandle);

pub(super) struct Patch {
    destination: Rect,
    uv: Rect,
}

pub fn paint(ctx: &egui::Context, rect: Rect, corner: f32) {
    let id = egui::Id::unique("sovereign-frame-texture");
    let cached = ctx.data(|data| data.get_temp::<FrameTexture>(id));
    let texture = cached.unwrap_or_else(|| {
        let value = FrameTexture(brand_art::texture(ctx, "sovereign-frame", FRAME));
        ctx.data_mut(|data| data.insert_temp(id, value.clone()));
        value
    });
    let [width, height] = texture.0.size();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, id));
    for patch in patches(rect, egui::vec2(width as f32, height as f32), corner) {
        painter.image(texture.0.id(), patch.destination, patch.uv, Color32::WHITE);
    }
}

fn patches(rect: Rect, source: Vec2, corner: f32) -> Vec<Patch> {
    let corner = corner.min(rect.width().min(rect.height()) * 0.49);
    let x = [
        rect.left(),
        rect.left() + corner,
        rect.right() - corner,
        rect.right(),
    ];
    let y = [
        rect.top(),
        rect.top() + corner,
        rect.bottom() - corner,
        rect.bottom(),
    ];
    let u = [
        0.,
        SOURCE_MARGIN / source.x,
        1. - SOURCE_MARGIN / source.x,
        1.,
    ];
    let v = [
        0.,
        SOURCE_MARGIN / source.y,
        1. - SOURCE_MARGIN / source.y,
        1.,
    ];
    let mut result = Vec::with_capacity(8);
    for row in 0..3 {
        for col in 0..3 {
            if row == 1 && col == 1 {
                continue;
            }
            result.push(Patch {
                destination: Rect::from_min_max(pos2(x[col], y[row]), pos2(x[col + 1], y[row + 1])),
                uv: Rect::from_min_max(pos2(u[col], v[row]), pos2(u[col + 1], v[row + 1])),
            });
        }
    }
    result
}

#[cfg(test)]
#[path = "skin_frame_tests.rs"]
mod tests;
