use eframe::egui;

const PORTRAIT: &[u8] = include_bytes!("../../assets/slimeybeats-dragon-portrait.png");
const ICON: &[u8] = include_bytes!("../../assets/slimeybeats-icon.png");
const RIBBONS: &[u8] = include_bytes!("../../assets/sovereign-ambient-ribbons.png");
const CREST: &[u8] = include_bytes!("../../assets/sovereign-amethyst-crest.png");

pub struct BrandArt {
    portrait: egui::TextureHandle,
    icon: egui::TextureHandle,
    ribbons: egui::TextureHandle,
    crest: egui::TextureHandle,
}
pub(super) fn texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> egui::TextureHandle {
    let image = image::load_from_memory(bytes)
        .expect("Validated embedded SlimeDragon artwork")
        .to_rgba8();
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    );
    ctx.load_texture(name, color, egui::TextureOptions::LINEAR)
}
impl BrandArt {
    pub fn new(ctx: &egui::Context) -> Self {
        Self {
            portrait: texture(ctx, "slimeybeats-portrait", PORTRAIT),
            icon: texture(ctx, "slimeybeats-icon", ICON),
            ribbons: texture(ctx, "sovereign-ribbons", RIBBONS),
            crest: texture(ctx, "sovereign-crest", CREST),
        }
    }
    pub fn show(&self, ui: &mut egui::Ui, size: f32) {
        ui.add(egui::Image::new(&self.portrait).fit_to_exact_size(egui::vec2(size, size)));
    }
    pub fn show_icon(&self, ui: &mut egui::Ui, size: f32) {
        ui.add(egui::Image::new(&self.icon).fit_to_exact_size(egui::vec2(size, size)));
    }
    pub fn show_ribbons(&self, ui: &egui::Ui, rect: egui::Rect, opacity: f32) {
        self.show_ribbons_at(ui, rect, opacity, 0.66);
    }
    pub fn show_ribbons_at(&self, ui: &egui::Ui, rect: egui::Rect, opacity: f32, anchor: f32) {
        let [width, height] = self.ribbons.size();
        let source_ratio = width as f32 / height as f32;
        let target_ratio = rect.width() / rect.height().max(1.);
        let uv = if target_ratio > source_ratio {
            let span = source_ratio / target_ratio;
            let top = (anchor - span * 0.5).clamp(0., 1. - span);
            egui::Rect::from_min_max(egui::pos2(0., top), egui::pos2(1., top + span))
        } else {
            let span = target_ratio / source_ratio;
            egui::Rect::from_min_max(
                egui::pos2((1. - span) * 0.5, 0.),
                egui::pos2((1. + span) * 0.5, 1.),
            )
        };
        ui.painter().with_clip_rect(rect).image(
            self.ribbons.id(),
            rect,
            uv,
            egui::Color32::WHITE.gamma_multiply(opacity.clamp(0., 1.)),
        );
    }
    pub fn paint_crest(&self, painter: &egui::Painter, center: egui::Pos2, width: f32) {
        let uv = egui::Rect::from_min_max(
            egui::pos2(34. / 1774., 126. / 887.),
            egui::pos2(1743. / 1774., 705. / 887.),
        );
        let height = width * 579. / 1709.;
        let rect = egui::Rect::from_center_size(center, egui::vec2(width, height));
        painter.image(self.crest.id(), rect, uv, egui::Color32::WHITE);
    }
}
pub fn window_icon() -> egui::IconData {
    let image = image::load_from_memory(ICON)
        .expect("Validated embedded SlimeDragon emblem")
        .resize_exact(64, 64, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    egui::IconData {
        width: 64,
        height: 64,
        rgba: image.into_raw(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn emblem_has_real_transparency_and_clear_padding() {
        let image = image::load_from_memory(ICON).unwrap().to_rgba8();
        assert!(image.width() >= 512 && image.height() >= 512);
        for (x, y) in [
            (0, 0),
            (image.width() - 1, 0),
            (0, image.height() - 1),
            (image.width() - 1, image.height() - 1),
        ] {
            // Generated PNG padding may carry one alpha quantization step (under 0.4% opacity).
            assert!(image.get_pixel(x, y)[3] <= 1);
        }
        let visible = image.pixels().filter(|p| p[3] > 128).count();
        assert!(visible > image.width() as usize * image.height() as usize / 5);
        assert!(visible < image.width() as usize * image.height() as usize * 9 / 10);
        assert_eq!(window_icon().rgba.len(), 64 * 64 * 4);
    }
}
