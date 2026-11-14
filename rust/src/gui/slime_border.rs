//! ImageGen slime artwork, sliced so hanging drops keep their natural proportions.
use super::brand_art;
use eframe::egui::{self, Color32, Rect, Vec2, pos2, vec2};

const SLIME_PNG: &[u8] = include_bytes!("../../assets/slimeybeats-slime-frame.png");
const SIDE_GUTTER: f32 = 14.;
const BOTTOM_GUTTER: f32 = 8.;
const SHOULDER_INSET: f32 = 128.;
const SOURCE_RIM_HEIGHT: f32 = 48.;
const CROWN_HEIGHT: f32 = 64.;
const CROWN_SOURCE_LEFT: f32 = 450.;
const CROWN_SOURCE_RIGHT: f32 = 1260.;
const CROWN_SOURCE_HEIGHT: f32 = 170.;

#[derive(Clone)]
struct SlimeTexture(egui::TextureHandle);

struct Patch {
    destination: Rect,
    uv: Rect,
}
impl Patch {
    fn new(destination: Rect, source: Rect, size: Vec2) -> Self {
        Self {
            destination,
            uv: Rect::from_min_max(
                pos2(source.min.x / size.x, source.min.y / size.y),
                pos2(source.max.x / size.x, source.max.y / size.y),
            ),
        }
    }
}

/// Paint only: no Areas, widgets, hit regions, timers or repaint requests.
pub fn paint(ctx: &egui::Context, frame: Rect, header_gap: [f32; 2]) {
    let id = egui::Id::unique("slime-image-border-texture");
    let cached = ctx.data(|data| data.get_temp::<SlimeTexture>(id));
    let texture = cached.unwrap_or_else(|| {
        let texture = SlimeTexture(brand_art::texture(
            ctx,
            "slimeybeats-slime-frame",
            SLIME_PNG,
        ));
        ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
        texture
    });
    let [width, height] = texture.0.size();
    let size = vec2(width as f32, height as f32);
    let painter = ctx
        .layer_painter(egui::LayerId::new(super::theme::DECORATION_ORDER, id))
        .with_clip_rect(frame);
    if let Some(crown) = crown_patch(frame, header_gap, size) {
        for right in [false, true] {
            painter.add(egui::Shape::mesh(shoulder_mesh(
                texture.0.id(),
                &crown,
                size,
                right,
            )));
        }
        painter.image(texture.0.id(), crown.destination, crown.uv, Color32::WHITE);
    }
}

fn crown_patch(frame: Rect, header_gap: [f32; 2], source: Vec2) -> Option<Patch> {
    let left = header_gap[0].max(frame.left() + SIDE_GUTTER);
    let right = header_gap[1].min(frame.right() - SIDE_GUTTER);
    if right - left < 120. {
        return None;
    }
    let crop = Rect::from_min_max(
        pos2(CROWN_SOURCE_LEFT, 0.),
        pos2(CROWN_SOURCE_RIGHT, CROWN_SOURCE_HEIGHT),
    );
    let full_width = source.x - SHOULDER_INSET * 2.;
    let height = CROWN_HEIGHT.min((frame.height() - 4.).max(0.));
    if height < 8. {
        return None;
    }
    let scale = (height / crop.height()).min((right - left - 16.) / full_width);
    let size = crop.size() * scale;
    let full_left = (frame.center().x - full_width * scale * 0.5)
        .clamp(left + 8., (right - 8. - full_width * scale).max(left + 8.));
    let destination = Rect::from_min_size(
        pos2(
            full_left + (CROWN_SOURCE_LEFT - SHOULDER_INSET) * scale,
            frame.top() - 1.,
        ),
        size,
    );
    Some(Patch::new(destination, crop, source))
}

fn shoulder_mesh(texture: egui::TextureId, crown: &Patch, source: Vec2, right: bool) -> egui::Mesh {
    let scale = crown.destination.height() / CROWN_SOURCE_HEIGHT;
    let (src_left, src_right, x_left, x_right, h_left, h_right, a_left, a_right) = if right {
        (
            CROWN_SOURCE_RIGHT,
            source.x - SHOULDER_INSET,
            crown.destination.right(),
            crown.destination.right() + (source.x - SHOULDER_INSET - CROWN_SOURCE_RIGHT) * scale,
            SOURCE_RIM_HEIGHT * scale,
            BOTTOM_GUTTER,
            255,
            0,
        )
    } else {
        (
            SHOULDER_INSET,
            CROWN_SOURCE_LEFT,
            crown.destination.left() - (CROWN_SOURCE_LEFT - SHOULDER_INSET) * scale,
            crown.destination.left(),
            BOTTOM_GUTTER,
            SOURCE_RIM_HEIGHT * scale,
            0,
            255,
        )
    };
    let top = crown.destination.top();
    let mut mesh = egui::Mesh::with_texture(texture);
    // Only droplet-free ribbon shoulders taper. The crown and its bulbs remain uniformly scaled.
    for (position, uv, alpha) in [
        (pos2(x_left, top), pos2(src_left / source.x, 0.), a_left),
        (pos2(x_right, top), pos2(src_right / source.x, 0.), a_right),
        (
            pos2(x_left, top + h_left),
            pos2(src_left / source.x, SOURCE_RIM_HEIGHT / source.y),
            a_left,
        ),
        (
            pos2(x_right, top + h_right),
            pos2(src_right / source.x, SOURCE_RIM_HEIGHT / source.y),
            a_right,
        ),
    ] {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: position,
            uv,
            color: Color32::from_white_alpha(alpha),
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;
    #[test]
    fn generated_frame_has_real_transparency_and_an_empty_center() {
        let image = image::load_from_memory(SLIME_PNG).unwrap().to_rgba8();
        assert!(image.width() >= 1500 && image.height() >= 900);
        assert!(image.pixels().any(|pixel| pixel[3] == 255));
        assert!(image.pixels().any(|pixel| pixel[3] > 0 && pixel[3] < 255));
        for y in image.height() / 4..image.height() * 3 / 4 {
            for x in image.width() / 4..image.width() * 3 / 4 {
                // 146 of 393,328 sampled center pixels carry one alpha quantization step (<0.4%).
                // The renderer never draws the center, so this is not an opaque background/UI.
                assert!(
                    image.get_pixel(x, y)[3] <= 1,
                    "The app opening must contain no baked UI/background"
                );
            }
        }
    }
    #[test]
    fn crown_preserves_drop_aspect_and_stays_in_the_empty_header() {
        let source = vec2(1586., 992.);
        for (width, gap) in [
            (1100., [470., 900.]),
            (1420., [470., 1220.]),
            (1920., [470., 1720.]),
        ] {
            let frame = Rect::from_min_size(Pos2::ZERO, vec2(width, 900.));
            let patch = crown_patch(frame, gap, source).unwrap();
            let source_size = patch.uv.size() * source;
            assert!(
                (patch.destination.width() / source_size.x
                    - patch.destination.height() / source_size.y)
                    .abs()
                    < 1e-5
            );
            assert!(patch.destination.left() >= gap[0] && patch.destination.right() <= gap[1]);
            assert!(patch.destination.bottom() <= CROWN_HEIGHT);
            for right in [false, true] {
                let mesh = shoulder_mesh(egui::TextureId::Managed(0), &patch, source, right);
                assert!(mesh.is_valid());
                assert!(
                    mesh.vertices
                        .iter()
                        .all(|v| v.pos.x >= gap[0] && v.pos.x <= gap[1] && v.pos.y <= CROWN_HEIGHT)
                );
            }
        }
        assert!(
            crown_patch(
                Rect::from_min_size(Pos2::ZERO, vec2(1100., 720.)),
                [500., 550.],
                source
            )
            .is_none()
        );
    }
}
