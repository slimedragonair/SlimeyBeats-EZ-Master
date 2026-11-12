use super::{
    app::MasterCraftApp, crystal_button, eq_panel, help, overview, settings_panel, skin_frame,
    skin_icons::Icon, slime_border, theme,
};
use eframe::egui::{self, Color32, Margin, Rect, RichText, pos2, vec2};
use std::{sync::atomic::Ordering, time::Duration};

const OUTER_INSET: egui::Vec2 = vec2(14., 12.);

impl eframe::App for MasterCraftApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.poll();
        let ctx = ui.ctx().clone();
        if !self.busy {
            let files = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .collect::<Vec<_>>()
            });
            if !files.is_empty() {
                self.add_paths(files);
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Space))
            && !ctx.egui_wants_keyboard_input()
            && self.current().is_some()
        {
            self.player.toggle();
        }
        let root = ui.available_rect_before_wrap();
        ui.painter().rect_filled(root, 0., theme::CANVAS);
        let mut inset = ui.new_child(egui::UiBuilder::new().max_rect(root.shrink2(OUTER_INSET)));
        let ui = &mut inset;
        let (header, gap) = self.sovereign_header(ui, &ctx);
        let footer = self.sovereign_footer(ui);
        let queue_max = (ui.available_width() - 290. - 430.).clamp(190., 420.);
        let queue = egui::Panel::left("queue")
            .default_size(250.)
            .min_size(190.)
            .max_size(queue_max)
            .resizable(true)
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(12)
                    .show(ui, |ui| super::queue_panel::show(self, ui));
            })
            .response
            .rect;
        let settings_max = (ui.available_width() - 430.).clamp(290., 450.);
        let settings = egui::Panel::right("settings")
            .default_size(332.)
            .min_size(290.)
            .max_size(settings_max)
            .resizable(true)
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(12)
                    .show(ui, |ui| settings_panel::show(self, ui, &ctx));
            })
            .response
            .rect;
        let workspace = ui.available_rect_before_wrap();
        egui::CentralPanel::default().show(ui, |ui| {
            ui.shrink_clip_rect(workspace);
            egui::Frame::new()
                .inner_margin(Margin {
                    left: 16,
                    right: 16,
                    top: 14,
                    bottom: 10,
                })
                .show(ui, |ui| self.sovereign_workspace(ui, &ctx));
        });
        // A single source coordinate grid connects the visible metal through all corners.
        for rect in [queue, settings, workspace, footer] {
            skin_frame::paint(&ctx, rect, 24.);
        }
        skin_frame::paint(&ctx, root, 28.);
        slime_border::paint(
            &ctx,
            Rect::from_min_max(root.min, pos2(root.right(), header.bottom())),
            gap,
        );
        slime_border::paint(
            &ctx,
            footer,
            [footer.center().x - 290., footer.center().x + 290.],
        );
        let ornament = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("sovereign-crests"),
        ));
        self.brand
            .paint_crest(&ornament, pos2(root.center().x, root.top() + 32.), 180.);
        self.brand.paint_crest(&ornament, footer.center(), 144.);
        self.sync_live_eq(&ctx);
        let ready = !self.busy && !self.eq_dirty;
        if let Some(result) = self
            .capture
            .as_mut()
            .and_then(|capture| capture.update(&ctx, ready))
        {
            self.capture = None;
            match result {
                Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Err(error) => {
                    eprintln!("{error}");
                    self.error = Some(error);
                }
            }
        }
        if self.busy || self.player.controls.playing.load(Ordering::Relaxed) {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }
}

impl MasterCraftApp {
    fn sovereign_header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> (Rect, [f32; 2]) {
        let mut gap = [0.; 2];
        let response = egui::Panel::top("brand").show(ui, |ui| {
            let background = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 84.));
            self.brand.show_ribbons_at(ui, background, 0.55, 0.36);
            egui::Frame::new()
                .inner_margin(Margin {
                    left: 16,
                    right: 16,
                    top: 6,
                    bottom: 6,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        self.brand.show_icon(ui, 64.);
                        ui.add_space(3.);
                        let identity = ui.vertical(|ui| {
                            ui.label(theme::display("SLIMEDRAGON AUDIO", 28.));
                            ui.horizontal(|ui| {
                                ui.label(theme::display("EZ-MASTER", 18.).extra_letter_spacing(2.));
                                ui.label(
                                    RichText::new(" |  SONG MASTERING & EQ")
                                        .size(12.)
                                        .extra_letter_spacing(1.8)
                                        .color(theme::MUTED),
                                );
                            });
                        });
                        gap[0] = identity.response.rect.right() + 22.;
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let analyze = ui
                                .add_enabled(
                                    !self.busy && !self.tracks.is_empty(),
                                    crystal_button::primary("Analyze")
                                        .icon(Icon::Analyze)
                                        .min_size(vec2(140., 42.)),
                                )
                                .on_hover_text(help::ANALYZE);
                            if analyze.clicked() {
                                self.start(true, ctx);
                            }
                            let version = ui.label(
                                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                    .size(12.)
                                    .color(theme::TEXT),
                            );
                            gap[1] = version.rect.left().min(analyze.rect.left()) - 22.;
                        });
                    });
                });
        });
        (response.response.rect, gap)
    }

    fn sovereign_footer(&mut self, ui: &mut egui::Ui) -> Rect {
        egui::Panel::bottom("status")
            .show(ui, |ui| {
                let background =
                    Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 42.));
                self.brand.show_ribbons_at(ui, background, 0.38, 0.76);
                egui::Frame::new()
                    .inner_margin(Margin {
                        left: 22,
                        right: 22,
                        top: 9,
                        bottom: 9,
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (light, _) =
                                ui.allocate_exact_size(vec2(12., 12.), egui::Sense::hover());
                            let color = if self.error.is_some() {
                                Color32::from_rgb(239, 136, 157)
                            } else if self.busy {
                                theme::AMETHYST
                            } else {
                                Color32::from_rgb(104, 230, 153)
                            };
                            ui.painter().circle_filled(
                                light.center(),
                                6.,
                                color.gamma_multiply(0.12),
                            );
                            ui.painter().circle_filled(light.center(), 3., color);
                            let status_width =
                                (ui.max_rect().center().x - 100. - ui.next_widget_position().x)
                                    .max(100.);
                            ui.allocate_ui_with_layout(
                                vec2(status_width, 18.),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_width(status_width);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&self.status)
                                                .size(13.)
                                                .color(theme::TEXT),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(&self.status);
                                },
                            );
                            ui.add_space(200.);
                            if self.busy {
                                ui.spinner();
                                ui.add(egui::ProgressBar::new(self.progress).desired_width(150.));
                                if ui.add(crystal_button::secondary("Cancel")).clicked() {
                                    self.cancel.store(true, Ordering::Relaxed);
                                    self.status = "Cancelling after current stage".into();
                                }
                            }
                        });
                    });
            })
            .response
            .rect
    }

    fn sovereign_workspace(&mut self, ui: &mut egui::Ui, _: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Frame::new()
                .fill(Color32::from_rgb(62, 29, 44))
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(error).color(Color32::from_rgb(255, 203, 220)));
                        if ui.add(crystal_button::secondary("Dismiss")).clicked() {
                            self.error = None;
                        }
                    });
                });
        }
        let name = self
            .selected
            .and_then(|i| self.tracks.get(i))
            .map(|track| {
                track
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_else(|| "Your next master starts here".into());
        ui.add(egui::Label::new(theme::display(&name, 26.)).truncate())
            .on_hover_text(&name);
        ui.add_space(4.);
        ui.horizontal_wrapped(|ui| {
            for (tab, label, icon) in [
                (0, "Overview", Icon::Home),
                (1, "Manual EQ", Icon::Sliders),
                (2, "Processing report", Icon::Report),
            ] {
                if ui
                    .add(
                        crystal_button::secondary(label)
                            .icon(icon)
                            .selected(self.tab == tab)
                            .min_size(vec2(0., 36.)),
                    )
                    .clicked()
                {
                    self.tab = tab;
                }
            }
        });
        ui.separator();
        let timeline = (self.tab == 0 || self.tab == 1) && self.current().is_some();
        let reserve = (if ui.available_width() >= 820. {
            65.
        } else {
            90.
        }) + if timeline {
            if ui.ctx().viewport_rect().height() < 800. {
                145.
            } else {
                165.
            }
        } else {
            0.
        };
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("workspace")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .max_height((ui.available_height() - reserve).max(110.));
        if let Some(offset) = self.capture_workspace_scroll {
            scroll = scroll.vertical_scroll_offset(offset);
        }
        scroll.show(ui, |ui| match self.tab {
            1 => eq_panel::show(self, ui, &ui.ctx().clone()),
            2 => overview::report(self, ui),
            _ => overview::show(self, ui),
        });
        ui.separator();
        if timeline {
            super::timeline_panel::show(self, ui);
            ui.separator();
        }
        super::transport::show(self, ui);
    }
}
