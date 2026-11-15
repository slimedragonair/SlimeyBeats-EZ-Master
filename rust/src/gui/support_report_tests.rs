use super::{
    support_report_state, test_fixture, theme,
    ui_test_input::{click, settled, text_rect},
    user_experience::HelpPage,
};
use eframe::egui;

fn action(
    ctx: &egui::Context,
    app: &mut super::app::MasterCraftApp,
    size: egui::Vec2,
    output: &egui::FullOutput,
    label: &str,
) -> Vec<egui::OutputCommand> {
    let pos = text_rect(output, label).unwrap().center();
    let event = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let down = super::ui_test_input::frame(
        ctx,
        app,
        size,
        vec![egui::Event::PointerMoved(pos), event(true)],
    );
    let up = super::ui_test_input::frame(ctx, app, size, vec![event(false)]);
    down.platform_output
        .commands
        .into_iter()
        .chain(up.platform_output.commands)
        .collect()
}

#[test]
fn generating_a_report_never_collects_private_session_data_or_sends_it() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::mastered(&ctx);
    let size = egui::vec2(1480., 940.);
    settled(&ctx, &mut app, size);
    app.tracks[0].path = "C:/Users/PRIVATE_CANARY/PRIVATE_SONG.wav".into();
    app.settings.out_dir = "C:/Users/PRIVATE_CANARY/PRIVATE_OUTPUT".into();
    app.error = Some("RAW_ERROR_CANARY with token=PRIVATE_TOKEN_CANARY".into());
    app.support_report.title = "Export stopped".into();
    app.support_report.details = "Click Export after changing the format.".into();
    app.experience.open_help(HelpPage::ReportProblem, &ctx);
    let output = settled(&ctx, &mut app, size);
    let commands = action(
        &ctx,
        &mut app,
        size,
        &output,
        "Generate local report preview",
    );
    assert!(app.support_report.preview.is_some());
    assert!(!app.support_report.reviewed);
    let text = app.support_report.preview.as_ref().unwrap().markdown();
    for secret in [
        "PRIVATE_CANARY",
        "PRIVATE_SONG",
        "PRIVATE_OUTPUT",
        "RAW_ERROR_CANARY",
        "PRIVATE_TOKEN_CANARY",
    ] {
        assert!(!text.contains(secret), "Leaked {secret}");
    }
    assert!(
        commands.is_empty(),
        "Generating a preview must not copy or open a URL"
    );
    assert_eq!(app.support_report.preview_text, text);
}
#[test]
fn report_save_requires_review_and_saves_the_exact_preview_without_overwrite() {
    let ctx = egui::Context::default();
    let app = test_fixture::app(&ctx);
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("report.md");
    let mut state = support_report_state::SupportReportState {
        title: "Test".into(),
        details: "Repro steps".into(),
        ..Default::default()
    };
    state.build(support_report_state::context(&app));
    let expected = state.preview_text.clone();
    state.save(&file);
    assert!(!file.exists());
    state.reviewed = true;
    state.save(&file);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), expected);
    state.title = "New description".into();
    state.build(support_report_state::context(&app));
    assert!(!state.reviewed);
    state.reviewed = true;
    state.save(&file);
    assert_eq!(std::fs::read_to_string(file).unwrap(), expected);
    assert!(state.error.is_some());
}
#[test]
fn recovery_and_reporting_help_actions_are_visible_at_supported_sizes() {
    for size in [egui::vec2(1100., 720.), egui::vec2(1480., 940.)] {
        for page in [HelpPage::Recovery, HelpPage::ReportProblem] {
            let ctx = egui::Context::default();
            theme::apply(&ctx);
            let mut app = test_fixture::app(&ctx);
            app.experience.open_help(page, &ctx);
            let output = settled(&ctx, &mut app, size);
            assert!(
                text_rect(&output, "Close help").is_some(),
                "Footer clipped at {size:?}"
            );
            assert!(text_rect(&output, "Recovery").is_some());
            assert!(text_rect(&output, "Report a problem").is_some());
        }
    }
}

#[test]
fn long_report_keeps_review_copy_save_and_both_sharing_routes_visible() {
    for size in [egui::vec2(1100., 720.), egui::vec2(1480., 940.)] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = test_fixture::app(&ctx);
        app.support_report.title = "A long report".into();
        app.support_report.details = "Steps that the user deliberately entered.\n".repeat(80);
        app.support_report
            .build(support_report_state::context(&app));
        app.experience.open_help(HelpPage::ReportProblem, &ctx);
        let output = settled(&ctx, &mut app, size);
        for label in [
            "I reviewed this preview and chose what to share",
            "Copy reviewed report",
            "Save reviewed report…",
            "Public issue form (public)",
            "Private security reporting",
            "Close help",
        ] {
            assert!(
                text_rect(&output, label).is_some(),
                "Missing or clipped {label} at {size:?}"
            );
        }
        assert!(
            action(&ctx, &mut app, size, &output, "Copy reviewed report").is_empty(),
            "Copy is gated before review"
        );
        let output = settled(&ctx, &mut app, size);
        let output = click(
            &ctx,
            &mut app,
            size,
            &output,
            "I reviewed this preview and chose what to share",
        );
        assert!(app.support_report.reviewed);
        let commands = action(&ctx, &mut app, size, &output, "Copy reviewed report");
        let expected = app.support_report.preview.as_ref().unwrap().markdown();
        assert!(
            commands
                .iter()
                .any(|c| matches!(c,egui::OutputCommand::CopyText(value) if value==expected)),
            "Copy must contain the exact reviewed preview"
        );
        assert!(
            commands
                .iter()
                .all(|c| !matches!(c, egui::OutputCommand::OpenUrl(_))),
            "Copy must not open or post to a destination"
        );
    }
}
