use super::{
    app::MasterCraftApp,
    crystal_button,
    guide_content::guide,
    theme,
    user_experience::{HelpPage, UserExperience},
};
use eframe::egui::{self, Id, RichText, vec2};

pub(super) fn show(app: &mut MasterCraftApp, ctx: &egui::Context) {
    app.experience.restore_pending_focus(ctx);
    app.experience.mcp.poll();
    if app.experience.welcome_open {
        welcome(&mut app.experience, ctx);
    }
    if app.experience.tour_open {
        tour(app, ctx);
    }
    if app.experience.help_page.is_some() {
        super::help_center::show(app, ctx);
    }
    app.experience.update(ctx);
}

pub(super) fn dialog_width(ctx: &egui::Context) -> f32 {
    (ctx.content_rect().width() - 80.).clamp(320., 720.)
}

pub(super) fn dialog_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
        .inner_margin(18)
        .stroke(egui::Stroke::new(1.2, theme::AMETHYST))
        .corner_radius(6)
}

pub(super) fn dialog(id: Id, name: &str, ctx: &egui::Context) -> egui::Modal {
    egui::Modal::new(id)
        .area(
            egui::Modal::default_area(id)
                .accessible_name(name)
                .role(egui::accesskit::Role::Dialog),
        )
        .frame(dialog_frame(ctx))
}

fn welcome(experience: &mut UserExperience, ctx: &egui::Context) {
    let mut action = None;
    let focus = experience.request_dialog_focus;
    let response =
        dialog(Id::unique("first-run-welcome"), &guide().welcome.title, ctx).show(ctx, |ui| {
            ui.set_width(dialog_width(ctx).min(560.));
            let content = &guide().welcome;
            ui.label(theme::display(&content.title, 26.));
            ui.add_space(8.);
            ui.label(&content.body);
            ui.add_space(5.);
            ui.label(
                RichText::new("8 short steps · You can leave at any time").color(theme::AMETHYST),
            );
            ui.add_space(8.);
            ui.label(RichText::new(&content.privacy_note).color(theme::MUTED));
            ui.add_space(16.);
            ui.horizontal_wrapped(|ui| {
                let start = ui.add(
                    crystal_button::primary(
                        if experience.preferences.tutorial_status
                            == super::interface_preferences::TutorialStatus::Started
                        {
                            "Continue the tour"
                        } else {
                            "Show me around"
                        },
                    )
                    .min_size(vec2(160., 42.)),
                );
                if focus {
                    start.request_focus();
                }
                if start.clicked() {
                    action = Some(true);
                }
                if ui
                    .add(
                        crystal_button::secondary("I'll explore on my own")
                            .min_size(vec2(180., 42.)),
                    )
                    .clicked()
                {
                    action = Some(false);
                }
            });
            ui.add_space(5.);
            ui.small("Your choice is remembered. Replay the tour from Help whenever you want.");
        });
    experience.request_dialog_focus = false;
    if action == Some(true) {
        experience.start_tour();
    } else if action == Some(false) || response.should_close() {
        experience.finish_tour(false);
    }
}

fn tour(app: &mut MasterCraftApp, ctx: &egui::Context) {
    let steps = &guide().steps;
    let index = app
        .experience
        .preferences
        .tutorial_step
        .min(steps.len() - 1);
    let step = &steps[index];
    let mut action = None;
    let response = dialog(Id::unique("guided-tour"), &format!("Quick tour: {}", step.title), ctx).show(ctx, |ui| {
        ui.set_width(dialog_width(ctx).min(620.));
        ui.horizontal(|ui| {
            theme::eyebrow(ui, "QUICK TOUR");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.label(format!("{} of {}", index + 1, steps.len())); });
        });
        ui.add(egui::ProgressBar::new((index + 1) as f32 / steps.len() as f32).desired_width(ui.available_width()));
        ui.add_space(10.);
        ui.label(theme::display(&step.title, 25.));
        ui.add_space(8.);
        ui.label(&step.body);
        ui.add_space(8.);
        ui.collapsing("Learn a little more", |ui| { ui.label(&step.detail); });
        ui.add_space(10.);
        if ui.add(crystal_button::secondary("See this screen")).on_hover_text("Pause the tour and show the relevant workspace. Nothing is processed or exported. Use Continue tour in the bottom bar to return.").clicked() { action = Some(TourAction::Show); }
        ui.add_space(10.);
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(index > 0, crystal_button::secondary("Back")).clicked() { action = Some(TourAction::Back); }
            let next = ui.add(crystal_button::primary(if index + 1 == steps.len() { "Finish tour" } else { "Next" }).min_size(vec2(108., 38.)));
            if app.experience.request_dialog_focus { next.request_focus(); }
            if next.clicked() { action = Some(TourAction::Next); }
            if ui.add(crystal_button::secondary("Leave tutorial")).clicked() { action = Some(TourAction::Leave); }
        });
    });
    app.experience.request_dialog_focus = false;
    match action {
        Some(TourAction::Back) => {
            app.experience.preferences.tutorial_step = index - 1;
            app.experience.persist(true, false);
        }
        Some(TourAction::Next) if index + 1 < steps.len() => {
            app.experience.preferences.tutorial_step = index + 1;
            app.experience.persist(true, false);
        }
        Some(TourAction::Next) => app.experience.finish_tour(true),
        Some(TourAction::Show) => {
            let tab = match step.desktop_target.as_str() {
                "eq" => 1,
                "stems" => 3,
                _ => 0,
            };
            app.activate_workspace(tab);
            app.experience.pause_tour();
        }
        Some(TourAction::Leave) => app.experience.finish_tour(false),
        None if response.should_close() => app.experience.finish_tour(false),
        None => {}
    }
}

enum TourAction {
    Back,
    Next,
    Show,
    Leave,
}

pub(super) fn mode_selector(experience: &mut UserExperience, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Controls");
        for (advanced, label) in [(false, "Basic"), (true, "Advanced")] {
            if ui.add(crystal_button::secondary(label).selected(experience.advanced() == advanced)).on_hover_text("Changes which controls are visible. Your current sound settings stay the same.").clicked() {
                experience.preferences.advanced = advanced;
                experience.persist(true, false);
            }
        }
    });
}

pub(super) fn help_button(app: &mut MasterCraftApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let response = ui
        .add(
            crystal_button::secondary("Help")
                .icon(super::skin_icons::Icon::Info)
                .min_size(vec2(92., 34.)),
        )
        .on_hover_text("Quick start, tutorial, terms and AI assistant setup · F1");
    if response.clicked() {
        response.request_focus();
        app.experience.open_help(HelpPage::Start, ctx);
    }
    if app.experience.tour_paused
        && ui
            .add(crystal_button::secondary(
                if ctx.viewport_rect().width() < 1250. {
                    "Tour"
                } else {
                    "Continue tour"
                },
            ))
            .on_hover_text("Continue the paused tutorial")
            .clicked()
    {
        app.experience.resume_tour();
    }
}
