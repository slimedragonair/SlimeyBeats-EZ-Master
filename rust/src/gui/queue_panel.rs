use super::{app::MasterCraftApp, crystal_button, skin_icons::Icon, theme};
use eframe::egui::{self, Color32, Rect, RichText, Sense, Stroke};

const DROP_EDGE: Color32 = Color32::from_rgb(132, 91, 164);
const QUEUE_MIN_HEIGHT: f32 = 176.;
const FOOTER_DETAILS_HEIGHT: f32 = 44.;
const FOOTER_GAP: f32 = 12.;

pub fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    ui.add_space(5.);
    ui.vertical_centered(|ui| {
        ui.label(theme::display("SESSION QUEUE", 16.));
    });
    queue_rule(ui);
    ui.label(theme::display(
        format!(
            "{} track{}",
            app.tracks.len(),
            if app.tracks.len() == 1 { "" } else { "s" }
        ),
        29.,
    ));
    ui.add_space(3.);
    queue_actions(app, ui);
    ui.add_space(4.);
    ui.separator();
    ui.add_space(5.);

    let portrait_size = (ui.available_width() - 24.).clamp(88., 190.).min(
        (ui.available_height() - QUEUE_MIN_HEIGHT - FOOTER_DETAILS_HEIGHT - FOOTER_GAP).max(0.),
    );
    let footer_height = portrait_size + FOOTER_DETAILS_HEIGHT;
    let show_footer = portrait_size >= 88.;
    let queue_height = (ui.available_height()
        - if show_footer {
            footer_height + FOOTER_GAP
        } else {
            0.
        })
    .max(64.);

    // Reserve the identity area before scrolling; a long queue cannot push it offscreen.
    egui::ScrollArea::vertical()
        .id_salt("queue-scroll")
        .max_height(queue_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.;
            for (i, track) in app.tracks.iter().enumerate() {
                let name = track.path.file_name().unwrap_or_default().to_string_lossy();
                let text = format!("{:02}  {name}\n{}", i + 1, track.status);
                if ui
                    .add(
                        crystal_button::secondary(&text)
                            .selected(app.selected == Some(i))
                            .min_size(egui::vec2(ui.available_width(), 66.))
                            .wrap(),
                    )
                    .on_hover_text(track.path.display().to_string())
                    .clicked()
                {
                    app.selected = Some(i);
                    app.loaded = None;
                    app.suggested_eq = None;
                    app.suggestion_note = None;
                    app.player.stop();
                }
            }
            if app.tracks.is_empty() {
                drop_zone(ui, app.busy, queue_height.min(204.));
            }
        });

    if show_footer {
        ui.add_space((ui.available_height() - footer_height).max(0.));
        ui.vertical_centered(|ui| {
            app.brand.show(ui, portrait_size);
            brand_rule(ui);
        });
    }
}

fn queue_actions(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    ui.add_enabled_ui(!app.busy, |ui| {
        if ui
            .add(
                crystal_button::primary("Add audio files")
                    .icon(Icon::Add)
                    .min_size(egui::vec2(ui.available_width(), 44.))
                    .wrap(),
            )
            .on_hover_text(
                "Add audio files to the session queue. You can also drop files onto the window.",
            )
            .clicked()
        {
            app.pick_files();
        }
        ui.columns(2, |columns| {
            let remove_width = columns[0].available_width();
            let clear_width = columns[1].available_width();
            if columns[0]
                .add(
                    crystal_button::secondary("Remove")
                        .icon(Icon::Trash)
                        .min_size(egui::vec2(remove_width, 36.)),
                )
                .on_hover_text(
                    "Remove the selected track from this session. The source file is preserved.",
                )
                .clicked()
                && let Some(i) = app.selected
            {
                app.tracks.remove(i);
                app.selected = if app.tracks.is_empty() {
                    None
                } else {
                    Some(i.min(app.tracks.len() - 1))
                };
                app.loaded = None;
                app.player.unload();
                app.eq_live.reset_source();
            }
            if columns[1]
                .add(
                    crystal_button::secondary("Clear")
                        .icon(Icon::Clear)
                        .min_size(egui::vec2(clear_width, 36.)),
                )
                .on_hover_text(
                    "Clear the session queue. Source audio and exported files are preserved.",
                )
                .clicked()
            {
                app.tracks.clear();
                app.selected = None;
                app.loaded = None;
                app.player.unload();
                app.eq_live.reset_source();
            }
        });
    });
}

fn drop_zone(ui: &mut egui::Ui, busy: bool, height: f32) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height.max(QUEUE_MIN_HEIGHT)),
        Sense::hover(),
    );
    let hovering_files = !busy && ui.ctx().input(|input| !input.raw.hovered_files.is_empty());
    let edge = if hovering_files {
        theme::AMETHYST
    } else {
        DROP_EDGE
    };
    let painter = ui.painter();
    painter.rect_filled(
        rect.shrink(1.),
        7.,
        if hovering_files {
            theme::SURFACE
        } else {
            theme::CANVAS
        },
    );
    let border = rect.shrink(1.);
    let cut = 8.;
    painter.extend(egui::Shape::dashed_line(
        &[
            border.left_top() + egui::vec2(cut, 0.),
            border.right_top() - egui::vec2(cut, 0.),
            border.right_top() + egui::vec2(0., cut),
            border.right_bottom() - egui::vec2(0., cut),
            border.right_bottom() - egui::vec2(cut, 0.),
            border.left_bottom() + egui::vec2(cut, 0.),
            border.left_bottom() - egui::vec2(0., cut),
            border.left_top() + egui::vec2(0., cut),
            border.left_top() + egui::vec2(cut, 0.),
        ],
        Stroke::new(if hovering_files { 1.5 } else { 1. }, edge),
        3.,
        3.,
    ));
    let center = egui::pos2(rect.center().x, rect.top() + 38.);
    let stroke = Stroke::new(2., edge);
    painter.line_segment(
        [center - egui::vec2(0., 13.), center + egui::vec2(0., 6.)],
        stroke,
    );
    painter.add(egui::Shape::line(
        vec![
            center + egui::vec2(-7., -1.),
            center + egui::vec2(0., 6.),
            center + egui::vec2(7., -1.),
        ],
        stroke,
    ));
    painter.add(egui::Shape::line(
        vec![
            center + egui::vec2(-15., 5.),
            center + egui::vec2(-15., 14.),
            center + egui::vec2(15., 14.),
            center + egui::vec2(15., 5.),
        ],
        stroke,
    ));
    let text_rect = Rect::from_min_max(
        egui::pos2(rect.left() + 10., rect.top() + 70.),
        rect.right_bottom() - egui::vec2(10., 12.),
    );
    let mut text_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
    text_ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 6.;
        ui.add(
            egui::Label::new(
                RichText::new("Drop audio here\nor add files to begin.")
                    .size(16.)
                    .color(theme::AMETHYST),
            )
            .wrap(),
        );
        ui.add_space(3.);
        ui.add(
            egui::Label::new(
                RichText::new("WAV · FLAC · MP3\nAIFF · M4A · OGG · more")
                    .size(13.)
                    .color(theme::MUTED),
            )
            .wrap(),
        );
    });
    response.on_hover_text(if busy {
        "Queue editing is unavailable while processing."
    } else {
        "Drop audio anywhere on the window, or choose Add audio files. Folders are accepted too."
    });
}

fn queue_rule(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 4.), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1., theme::BORDER),
    );
}

fn brand_rule(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.), Sense::hover());
    let gap = 77.;
    for (start, end) in [
        (rect.left_center(), rect.center() - egui::vec2(gap, 0.)),
        (rect.center() + egui::vec2(gap, 0.), rect.right_center()),
    ] {
        if end.x > start.x {
            ui.painter()
                .line_segment([start, end], Stroke::new(1., DROP_EDGE));
        }
    }
    ui.place(
        rect,
        egui::Label::new(theme::display("SLIMEDRAGON AUDIO", 11.).color(theme::MUTED)),
    );
}
