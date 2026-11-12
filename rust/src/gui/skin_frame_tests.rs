use super::*;
use egui::Pos2;
use image::RgbaImage;
use std::collections::VecDeque;

fn alpha(image: &RgbaImage, uv: Pos2) -> f32 {
    let x = (uv.x * image.width() as f32 - 0.5).clamp(0., image.width() as f32 - 1.);
    let y = (uv.y * image.height() as f32 - 0.5).clamp(0., image.height() as f32 - 1.);
    let (ix, iy) = (x.floor() as u32, y.floor() as u32);
    let (tx, ty) = (x - ix as f32, y - iy as f32);
    let pixel = |a: u32, b: u32| {
        image.get_pixel(a.min(image.width() - 1), b.min(image.height() - 1))[3] as f32 / 255.
    };
    let upper = pixel(ix, iy) * (1. - tx) + pixel(ix + 1, iy) * tx;
    let lower = pixel(ix, iy + 1) * (1. - tx) + pixel(ix + 1, iy + 1) * tx;
    upper * (1. - ty) + lower * ty
}

fn connected_corner(
    image: &RgbaImage,
    slices: &[Patch],
    origin: Pos2,
    side: usize,
    ppp: f32,
    right: bool,
    bottom: bool,
) -> bool {
    let mut mask = vec![false; side * side];
    for y in 0..side {
        for x in 0..side {
            let point = origin + egui::vec2((x as f32 + 0.5) / ppp, (y as f32 + 0.5) / ppp);
            let mut coverage = 0.;
            for patch in slices {
                if patch.destination.contains(point) {
                    let fraction = (point - patch.destination.min) / patch.destination.size();
                    let uv = patch.uv.min + patch.uv.size() * fraction;
                    let value = alpha(image, uv);
                    coverage += value * (1. - coverage);
                }
            }
            mask[y * side + x] = coverage >= 0.15;
        }
    }
    let mut seen = vec![false; mask.len()];
    let mut queue = VecDeque::new();
    let horizontal = if right { 0 } else { side - 1 };
    let vertical = if bottom { 0 } else { side - 1 };
    for y in 0..side {
        let i = y * side + horizontal;
        if mask[i] {
            queue.push_back((horizontal, y));
            seen[i] = true;
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        if y == vertical {
            return true;
        }
        for (nx, ny) in [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ] {
            if nx < side && ny < side {
                let i = ny * side + nx;
                if mask[i] && !seen[i] {
                    seen[i] = true;
                    queue.push_back((nx, ny));
                }
            }
        }
    }
    false
}

#[test]
fn visible_material_connects_all_four_corners_at_multiple_sizes_and_dpi() {
    let image = image::load_from_memory(FRAME).unwrap().to_rgba8();
    let source = egui::vec2(image.width() as f32, image.height() as f32);
    for size in [
        egui::vec2(1100., 720.),
        egui::vec2(1480., 940.),
        egui::vec2(1920., 1080.),
    ] {
        let frame = Rect::from_min_size(Pos2::ZERO, size);
        let slices = patches(frame, source, 28.);
        for ppp in [1., 1.25, 2.] {
            let extent = 52.;
            let side = (extent * ppp) as usize;
            for (right, bottom) in [(false, false), (true, false), (false, true), (true, true)] {
                let origin = pos2(
                    if right { size.x - extent } else { 0. },
                    if bottom { size.y - extent } else { 0. },
                );
                assert!(
                    connected_corner(&image, &slices, origin, side, ppp, right, bottom),
                    "Disconnected material: {size:?}/{ppp}, right={right}, bottom={bottom}"
                );
            }
        }
    }
}

#[test]
fn slices_are_bounded_and_leave_the_content_open() {
    let source = egui::vec2(1586., 992.);
    let frame = Rect::from_min_size(Pos2::ZERO, egui::vec2(1480., 940.));
    let slices = patches(frame, source, 28.);
    assert_eq!(slices.len(), 8);
    for patch in slices {
        assert!(frame.contains_rect(patch.destination));
        assert!(!patch.destination.contains(frame.center()));
        assert!(Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)).contains_rect(patch.uv));
    }
}

#[test]
fn pixel_connectivity_rejects_a_twelve_point_side_offset() {
    let image = image::load_from_memory(FRAME).unwrap().to_rgba8();
    let frame = Rect::from_min_size(Pos2::ZERO, egui::vec2(1480., 940.));
    let mut slices = patches(
        frame,
        egui::vec2(image.width() as f32, image.height() as f32),
        28.,
    );
    for patch in &mut slices {
        if patch.destination.height() > patch.destination.width() {
            patch.destination = patch.destination.translate(egui::vec2(12., 0.));
        }
    }
    assert!(!connected_corner(
        &image,
        &slices,
        Pos2::ZERO,
        65,
        1.25,
        false,
        false
    ));
}
