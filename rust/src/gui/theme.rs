use eframe::egui::{self, Color32, FontFamily, FontId, RichText, Stroke, TextStyle};
pub const SLIME: Color32 = Color32::from_rgb(112, 220, 207);
pub const AMETHYST: Color32 = Color32::from_rgb(210, 166, 250);
pub const CANVAS: Color32 = Color32::from_rgb(14, 11, 21);
pub const BONE: Color32 = Color32::from_rgb(229, 210, 179);
pub const MUTED: Color32 = Color32::from_rgb(190, 179, 205);
pub const SURFACE: Color32 = Color32::from_rgb(25, 20, 34);
pub const BORDER: Color32 = Color32::from_rgb(98, 82, 116);
pub const SILVER: Color32 = Color32::from_rgb(173, 158, 189);
pub const TEXT: Color32 = Color32::from_rgb(245, 239, 252);
pub const DISPLAY_FONT: &str = "slimeybeats-display";
// Artwork sits above the base panels, while popups and modal content stay on top.
pub const DECORATION_ORDER: egui::Order = egui::Order::Middle;

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = Color32::from_rgb(18, 14, 25);
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = CANVAS;
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.weak_text_color = Some(MUTED);
    style.visuals.disabled_alpha = 0.72;
    style.visuals.selection.bg_fill = Color32::from_rgb(100, 39, 150);
    style.visuals.selection.stroke = Stroke::new(1.5, Color32::WHITE);
    style.visuals.hyperlink_color = AMETHYST;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1., BORDER);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1., TEXT);
    for (visuals, fill, border) in [
        (
            &mut style.visuals.widgets.inactive,
            Color32::from_rgb(34, 28, 44),
            SILVER,
        ),
        (
            &mut style.visuals.widgets.hovered,
            Color32::from_rgb(52, 35, 69),
            AMETHYST,
        ),
        (
            &mut style.visuals.widgets.active,
            Color32::from_rgb(74, 39, 104),
            Color32::from_rgb(232, 195, 255),
        ),
        (
            &mut style.visuals.widgets.open,
            Color32::from_rgb(47, 30, 63),
            AMETHYST,
        ),
    ] {
        visuals.bg_fill = fill;
        visuals.weak_bg_fill = fill;
        visuals.bg_stroke = Stroke::new(1., border);
        visuals.fg_stroke = Stroke::new(1.4, Color32::WHITE);
        visuals.corner_radius = egui::CornerRadius::same(3);
        visuals.expansion = 0.;
    }
    style.spacing.item_spacing = egui::vec2(8., 7.);
    style.spacing.button_padding = egui::vec2(12., 6.);
    style.spacing.interact_size.y = 32.;
    style.spacing.slider_rail_height = 5.;
    style.spacing.slider_width = 160.;
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.scroll.bar_width = 7.;
    style.spacing.scroll.bar_inner_margin = 6.;
    style.spacing.scroll.bar_outer_margin = 1.;
    style.spacing.scroll.handle_min_length = 24.;
    style.spacing.scroll.foreground_color = true;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(16.));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(16.));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(13.));
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(24., FontFamily::Name(DISPLAY_FONT.into())),
    );
    ctx.set_global_style(style);
}
fn install_fonts(ctx: &egui::Context) {
    // Load installed faces at runtime; system font files are not bundled or redistributed.
    let mut fonts = egui::FontDefinitions::default();
    #[cfg(target_os = "linux")]
    let ui_paths = ["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"];
    #[cfg(not(target_os = "linux"))]
    let ui_paths = ["C:/Windows/Fonts/segoeui.ttf"];
    if let Some(bytes) = ui_paths.iter().find_map(|path| std::fs::read(path).ok()) {
        fonts
            .font_data
            .insert("system-ui".into(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "system-ui".into());
    }
    let mut display = Vec::new();
    #[cfg(target_os = "linux")]
    let display_paths = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSerif-Regular.ttf",
    ];
    #[cfg(not(target_os = "linux"))]
    let display_paths = ["C:/Windows/Fonts/georgia.ttf", "C:/Windows/Fonts/times.ttf"];
    for path in display_paths {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "system-display".into(),
                egui::FontData::from_owned(bytes).into(),
            );
            display.push("system-display".into());
            break;
        }
    }
    // Keep the named family usable even when neither installed serif is available.
    display.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name(DISPLAY_FONT.into()), display);
    ctx.set_fonts(fonts);
}

pub fn display(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text)
        .font(FontId::new(size, FontFamily::Name(DISPLAY_FONT.into())))
        .color(AMETHYST)
}

pub fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(display(text, 14.).extra_letter_spacing(1.4).color(TEXT));
}

pub fn primary(text: impl Into<egui::WidgetText>) -> super::crystal_button::CrystalButton {
    super::crystal_button::primary(text)
}
