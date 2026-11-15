//! Explicit local support draft → immutable preview → reviewed copy/save.
use super::{app::MasterCraftApp, crystal_button, theme};
use crate::support_report::{IssueCategory, PRIVATE_SECURITY_REPORT_URL};
use eframe::egui;

const CATEGORIES: [(IssueCategory, &str); 11] = [
    (IssueCategory::Crash, "Crash or recovery"),
    (IssueCategory::FreezeOrSlow, "Freeze or slow processing"),
    (IssueCategory::AudioPlayback, "Audio playback"),
    (IssueCategory::ImportOrExport, "Import or export"),
    (IssueCategory::MasteringOrEq, "Mastering or EQ"),
    (IssueCategory::Stems, "Stems"),
    (IssueCategory::OptionalToolSetup, "Optional tool setup"),
    (IssueCategory::Updates, "Application updates"),
    (IssueCategory::Recovery, "Autosave and recovery"),
    (IssueCategory::Accessibility, "Accessibility"),
    (IssueCategory::Other, "Other"),
];

pub(super) fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui, _ctx: &egui::Context) {
    ui.strong("Report a problem, with control over what you share");
    ui.label("Write a short description, generate a local preview, then review every line before copying or saving. Audio, project contents and raw error logs are never attached automatically.");
    let state = &mut app.support_report;
    let mut changed = false;
    let category = ui.label("Problem type");
    egui::ComboBox::from_id_salt("support-category")
        .selected_text(
            CATEGORIES
                .iter()
                .find(|(c, _)| *c == state.category)
                .unwrap()
                .1,
        )
        .show_ui(ui, |ui| {
            for (category, label) in CATEGORIES {
                changed |= ui
                    .selectable_value(&mut state.category, category, label)
                    .changed();
            }
        })
        .response
        .labelled_by(category.id);
    ui.label("Short title");
    changed |= ui
        .add(
            egui::TextEdit::singleline(&mut state.title)
                .char_limit(120)
                .desired_width(f32::INFINITY)
                .hint_text("For example: export stopped after changing format"),
        )
        .changed();
    ui.label("What happened? What did you expect?");
    changed|=ui.add(egui::TextEdit::multiline(&mut state.details).char_limit(4000).desired_rows(5).desired_width(f32::INFINITY).hint_text("Steps to reproduce, and whether it happens every time. Leave out private names, paths and credentials.")).changed();
    if changed {
        state.invalidate();
    }
    if ui
        .add(crystal_button::primary("Generate local report preview"))
        .clicked()
    {
        let context = super::support_report_state::context(app);
        app.support_report.build(context);
    }
    let state = &mut app.support_report;
    if let Some(report) = &state.preview {
        ui.separator();
        let heading = ui.strong("This preview is the complete report");
        if state.reveal_preview {
            heading.scroll_to_me(Some(egui::Align::TOP));
            state.reveal_preview = false;
        }
        ui.small("Review the complete text below. The report's privacy notice explains what is included and what still needs your review.");
        if report.text_was_changed() {
            ui.colored_label(theme::AMETHYST,"The privacy filter changed your typed text. Check that it still explains the problem.");
        }
        ui.add(
            egui::TextEdit::multiline(&mut state.preview_text)
                .desired_rows(12)
                .desired_width(f32::INFINITY)
                .interactive(false)
                .text_color(theme::TEXT)
                .font(egui::TextStyle::Monospace),
        );
    }
}

#[derive(Default)]
struct Actions {
    copy: bool,
    save: bool,
}
fn footer(
    ui: &mut egui::Ui,
    reviewed: &mut bool,
    ready: bool,
    message: &str,
    error: Option<&str>,
) -> Actions {
    let mut actions = Actions::default();
    if ready {
        ui.checkbox(reviewed, "I reviewed this preview and chose what to share");
        ui.horizontal_wrapped(|ui| {
            actions.copy = ui
                .add_enabled(*reviewed, crystal_button::primary("Copy reviewed report"))
                .clicked();
            actions.save = ui
                .add_enabled(
                    *reviewed,
                    crystal_button::secondary("Save reviewed report…"),
                )
                .clicked();
        });
    }
    if !message.is_empty() {
        ui.label(message);
    }
    if let Some(error) = error {
        ui.colored_label(theme::BONE, error);
    }
    ui.scope_builder(
        egui::UiBuilder::new().scope_id(egui::Id::unique("support-sharing-links")),
        |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.hyperlink_to(
                    "Public issue form (public)",
                    "https://github.com/slimedragonair/SlimeyBeats-EZ-Master/issues/new",
                );
                ui.hyperlink_to("Private security reporting", PRIVATE_SECURITY_REPORT_URL);
            });
            ui.small(
                "These links open forms. No report or recording is attached or sent automatically.",
            );
        },
    );
    actions
}
pub(super) fn footer_height(app: &MasterCraftApp, ui: &mut egui::Ui) -> f32 {
    let state = &app.support_report;
    let mut reviewed = state.reviewed;
    let mut measure = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("support-footer-measure")
            .sizing_pass()
            .invisible(),
    );
    measure.disable();
    footer(
        &mut measure,
        &mut reviewed,
        state.preview.is_some(),
        &state.message,
        state.error.as_deref(),
    );
    measure.min_rect().height() + ui.spacing().item_spacing.y
}
pub(super) fn show_footer(app: &mut MasterCraftApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let state = &mut app.support_report;
    let actions = footer(
        ui,
        &mut state.reviewed,
        state.preview.is_some(),
        &state.message,
        state.error.as_deref(),
    );
    if actions.copy
        && let Some(report) = &state.preview
    {
        ctx.copy_text(report.markdown().into());
        state.message = "Report copied. Paste it only where you choose to share it.".into();
    }
    if actions.save
        && let Some(path) = rfd::FileDialog::new()
            .set_title("Save local support report")
            .set_file_name("SlimeyBeats-support-report.md")
            .add_filter("Markdown report", &["md"])
            .save_file()
    {
        state.save(&path);
    }
}
