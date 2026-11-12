use super::{
    app::MasterCraftApp, crystal_button, crystal_slider::CrystalSlider, help, skin_icons::Icon,
    theme,
};
use eframe::egui::{self, RichText};
use std::sync::atomic::Ordering;
pub fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    ui.add_space(5.);
    ui.add_enabled_ui(app.current().is_some(), |ui| {
        transport_row(ui, |ui| {
            let playing = app.player.controls.playing.load(Ordering::Relaxed);
            if ui
                .add(
                    crystal_button::secondary(if playing { "Pause" } else { "Play" })
                        .icon(if playing { Icon::Pause } else { Icon::Play }),
                )
                .clicked()
            {
                if app.player.seconds() >= app.player.duration {
                    app.player.seek(0.);
                }
                app.player.toggle();
            }
            if ui
                .add(crystal_button::secondary("Stop").icon(Icon::Stop))
                .clicked()
            {
                app.player.stop();
            }
            let after = app.player.controls.after.load(Ordering::Relaxed);
            if ui
                .add(
                    crystal_button::secondary(if app.tab == 1 {
                        "A · Pre EQ"
                    } else {
                        "A · Original"
                    })
                    .selected(!after),
                )
                .on_hover_text(if app.tab == 1 { help::AB_EQ } else { help::AB })
                .clicked()
            {
                app.player.controls.after.store(false, Ordering::Relaxed);
            }
            let has_after = app.current().is_some_and(|(_, r)| r.after.is_some());
            if ui
                .add_enabled(
                    has_after,
                    crystal_button::secondary(if app.tab == 1 {
                        "B · Live EQ"
                    } else {
                        "B · Master"
                    })
                    .selected(after),
                )
                .on_hover_text(if app.tab == 1 { help::AB_EQ } else { help::AB })
                .clicked()
            {
                app.player.controls.after.store(true, Ordering::Relaxed);
            }
            ui.checkbox(&mut app.match_levels, "Loudness match")
                .on_hover_text(help::LEVEL_MATCH);
            volume_slider(ui, &mut app.volume).on_hover_text(help::VOLUME);
            clock_label(
                ui,
                app.player.seconds().min(app.player.duration),
                app.player.duration,
            );
        });
    });
    app.player
        .controls
        .matched
        .store(app.match_levels, Ordering::Relaxed);
    app.player
        .controls
        .volume
        .store(app.volume.to_bits(), Ordering::Relaxed);
}

fn transport_row<R>(
    ui: &mut egui::Ui,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 7.;
        ui.spacing_mut().slider_width = 84.;
        // Let the wrapping layout move controls as units, rather than breaking their text.
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        add(ui)
    })
}

fn clock_label(ui: &mut egui::Ui, elapsed: f64, duration: f64) -> egui::Response {
    ui.add(
        egui::Label::new(
            RichText::new(format!("{} / {}", clock(elapsed), clock(duration)))
                .monospace()
                .size(13.)
                .color(theme::MUTED),
        )
        .wrap_mode(egui::TextWrapMode::Extend),
    )
}

fn volume_slider(ui: &mut egui::Ui, volume: &mut f32) -> egui::Response {
    // Slider is a composite horizontal layout. Reserve its measured size before
    // painting so its rail and label move to the next transport row together.
    let mut measured_volume = *volume;
    let mut measure = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("volume-width-measure")
            .sizing_pass()
            .invisible(),
    );
    let size = measure
        .add(
            CrystalSlider::new(&mut measured_volume, 0.0..=1.0)
                .text("Volume")
                .show_value(false),
        )
        .rect
        .size();
    ui.add_sized(
        size,
        CrystalSlider::new(volume, 0.0..=1.0)
            .text("Volume")
            .show_value(false),
    )
}

fn clock(seconds: f64) -> String {
    format!("{}:{:02}", seconds as u64 / 60, seconds as u64 % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_wraps_as_one_label_at_every_remaining_row_width() {
        for remaining in [1., 15., 30., 60., 90., 140.] {
            let ctx = egui::Context::default();
            theme::apply(&ctx);
            let mut bounds = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320., 160.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let right = ui.max_rect().right();
                    let row = transport_row(ui, |ui| {
                        let prefix = ui
                            .allocate_exact_size(
                                egui::vec2(ui.available_width() - remaining, 32.),
                                egui::Sense::hover(),
                            )
                            .0;
                        let clock = clock_label(ui, 0., 48.);
                        (prefix, clock.rect)
                    });
                    bounds = Some((right, row.inner.0, row.inner.1));
                },
            );
            let (right, prefix, clock) = bounds.unwrap();
            let rows = output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    egui::Shape::Text(text) if text.galley.text() == "0:00 / 0:48" => {
                        Some(text.galley.rows.len())
                    }
                    _ => None,
                });
            output.textures_delta.clear();
            assert_eq!(rows, Some(1), "clock at {remaining} points remaining");
            assert!(clock.right() <= right + 0.5);
            if remaining < clock.width() {
                assert!(
                    clock.top() >= prefix.bottom(),
                    "the whole clock must move to the next row"
                );
            }
        }
    }

    #[test]
    fn volume_rail_and_label_wrap_together_without_overflow() {
        for remaining in [1., 30., 90., 180.] {
            let ctx = egui::Context::default();
            theme::apply(&ctx);
            let mut volume = 0.5;
            let mut bounds = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320., 160.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let right = ui.max_rect().right();
                    let row = transport_row(ui, |ui| {
                        let prefix = ui
                            .allocate_exact_size(
                                egui::vec2(ui.available_width() - remaining, 32.),
                                egui::Sense::hover(),
                            )
                            .0;
                        (prefix, volume_slider(ui, &mut volume).rect)
                    });
                    bounds = Some((right, row.inner.0, row.inner.1));
                },
            );
            output.textures_delta.clear();
            let (right, prefix, slider) = bounds.unwrap();
            assert!(
                slider.right() <= right + 0.5,
                "volume overflow at {remaining} points remaining"
            );
            if remaining < slider.width() {
                assert!(slider.top() >= prefix.bottom());
            }
            assert_eq!(volume, 0.5);
        }
    }
}
