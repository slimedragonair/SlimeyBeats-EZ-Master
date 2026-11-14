//! Pointer input and geometry for headless tests of the actual native frame.
use super::app::MasterCraftApp;
use eframe::egui;

pub(super) fn frame(
    ctx: &egui::Context,
    app: &mut MasterCraftApp,
    viewport: egui::Vec2,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
            events,
            ..Default::default()
        },
        |ui| app.render_frame(ui),
    );
    output.textures_delta.clear();
    output
}

pub(super) fn settled(
    ctx: &egui::Context,
    app: &mut MasterCraftApp,
    viewport: egui::Vec2,
) -> egui::FullOutput {
    let mut output = frame(ctx, app, viewport, vec![]);
    for _ in 0..5 {
        output = frame(ctx, app, viewport, vec![]);
    }
    output
}

pub(super) fn text_rect(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    output.shapes.iter().find_map(|item| match &item.shape {
        egui::Shape::Text(text) if text.galley.text() == label => {
            let rect = item.shape.visual_bounding_rect();
            item.clip_rect.contains_rect(rect).then_some(rect)
        }
        _ => None,
    })
}

pub(super) fn click(
    ctx: &egui::Context,
    app: &mut MasterCraftApp,
    viewport: egui::Vec2,
    output: &egui::FullOutput,
    label: &str,
) -> egui::FullOutput {
    let pos = text_rect(output, label)
        .unwrap_or_else(|| panic!("Visible action not found: {label}"))
        .center();
    let event = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        ctx,
        app,
        viewport,
        vec![egui::Event::PointerMoved(pos), event(true)],
    );
    frame(ctx, app, viewport, vec![event(false)]);
    settled(ctx, app, viewport)
}
