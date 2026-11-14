use super::{
    guide_content::guide, interface_preferences::TutorialStatus, test_fixture, theme,
    ui_test_input::*, user_experience::UserExperience,
};
use eframe::egui;

#[test]
fn native_accessibility_tree_names_the_welcome_dialog_and_its_choices() {
    let folder = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    ctx.enable_accesskit();
    let mut app = test_fixture::app(&ctx);
    app.experience = UserExperience::load(folder.path().join("interface-preferences.json"));
    let output = settled(&ctx, &mut app, egui::vec2(1100., 720.));
    let tree = output
        .platform_output
        .accesskit_update
        .expect("Native accessibility must expose a tree");
    assert!(
        tree.nodes
            .iter()
            .any(|(_, node)| node.role() == egui::accesskit::Role::Dialog
                && node.label() == Some(guide().welcome.title.as_str()))
    );
    for label in ["Show me around", "I'll explore on my own"] {
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.role() == egui::accesskit::Role::Button
                    && node.label() == Some(label)),
            "Accessible button missing: {label}"
        );
    }
}

#[test]
fn closing_help_returns_keyboard_activation_to_the_help_trigger() {
    for escape in [false, true] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = test_fixture::app(&ctx);
        app.tab = 0;
        let viewport = egui::vec2(1100., 720.);
        let output = settled(&ctx, &mut app, viewport);
        let output = click(&ctx, &mut app, viewport, &output, "Help");
        if escape {
            frame(
                &ctx,
                &mut app,
                viewport,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            settled(&ctx, &mut app, viewport);
        } else {
            click(&ctx, &mut app, viewport, &output, "Close help");
        }
        assert!(!app.experience.has_modal());
        frame(
            &ctx,
            &mut app,
            viewport,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        settled(&ctx, &mut app, viewport);
        assert!(
            app.experience.help_page.is_some(),
            "Enter did not activate the restored Help trigger (escape={escape})"
        );
    }
}

#[test]
fn glossary_filtering_keeps_the_search_field_in_a_stable_position() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::app(&ctx);
    app.experience
        .open_help(super::user_experience::HelpPage::Terms, &ctx);
    let viewport = egui::vec2(1100., 720.);
    let full = settled(&ctx, &mut app, viewport);
    let before = text_rect(&full, "Find a term").unwrap();
    app.experience.search = "lufs".into();
    let filtered = settled(&ctx, &mut app, viewport);
    let after = text_rect(&filtered, "Find a term").unwrap();
    assert!(
        (before.top() - after.top()).abs() < 1.,
        "Search moved during filtering: {before:?} -> {after:?}"
    );
}

#[test]
fn real_welcome_buttons_and_tour_completion_remember_the_choice_without_audio_jobs() {
    for complete in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("interface-preferences.json");
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = test_fixture::app(&ctx);
        app.tab = 0;
        app.experience = UserExperience::load(path.clone());
        let original = serde_json::to_value(&app.settings).unwrap();
        let viewport = egui::vec2(1100., 720.);
        let mut output = settled(&ctx, &mut app, viewport);
        assert!(text_rect(&output, &guide().welcome.title).is_some());
        if complete {
            output = click(&ctx, &mut app, viewport, &output, "Show me around");
            for index in 0..guide().steps.len() {
                assert_eq!(app.experience.preferences.tutorial_step, index);
                output = click(
                    &ctx,
                    &mut app,
                    viewport,
                    &output,
                    if index + 1 == guide().steps.len() {
                        "Finish tour"
                    } else {
                        "Next"
                    },
                );
            }
        } else {
            click(&ctx, &mut app, viewport, &output, "I'll explore on my own");
        }
        assert!(!app.experience.has_modal());
        assert_eq!(
            app.experience.preferences.tutorial_status,
            if complete {
                TutorialStatus::Completed
            } else {
                TutorialStatus::Declined
            }
        );
        assert_eq!(serde_json::to_value(&app.settings).unwrap(), original);
        assert!(app.tracks.is_empty() && !app.busy && !app.studio.busy());
        assert!(!app.experience.mcp.busy());
        let mut restarted = UserExperience::load(path);
        restarted.offer_when_idle(true);
        assert!(!restarted.welcome_open);
        assert!(!folder.path().read_dir().unwrap().any(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_some_and(|value| value == "wav")
        }));
    }
}

#[test]
fn basic_and_advanced_switch_visibility_without_changing_custom_audio_choices() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::app(&ctx);
    app.tab = 0;
    app.settings.target_lufs = Some(-16.);
    app.settings.intensity = 1.123456;
    app.settings.formats = vec!["wav16".into(), "flac24".into()];
    app.settings.manual_eq[2].q = 2.3;
    let original = serde_json::to_value(&app.settings).unwrap();
    let viewport = egui::vec2(1480., 940.);
    let output = settled(&ctx, &mut app, viewport);
    assert!(text_rect(&output, "Custom target / integrated LUFS").is_none());
    assert!(text_rect(&output, "Review advanced").is_some());
    let output = click(&ctx, &mut app, viewport, &output, "Advanced");
    assert!(app.experience.advanced());
    assert_eq!(serde_json::to_value(&app.settings).unwrap(), original);
    click(&ctx, &mut app, viewport, &output, "Basic");
    assert!(!app.experience.advanced());
    assert_eq!(serde_json::to_value(&app.settings).unwrap(), original);
}

#[test]
fn compact_eq_keeps_the_selected_frequency_and_gain_visible_above_the_plot() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::mastered(&ctx);
    app.tab = 1;
    let viewport = egui::vec2(1100., 720.);
    let output = settled(&ctx, &mut app, viewport);
    for label in ["Band", "Frequency", "Gain / dB", "Help"] {
        let rect = text_rect(&output, label)
            .unwrap_or_else(|| panic!("Essential compact control hidden: {label}"));
        assert!(
            rect.bottom() < viewport.y && rect.right() < viewport.x,
            "{label}: {rect:?}"
        );
    }
    assert!(text_rect(&output, "Q / width").is_none());
    assert!(app.error.is_none(), "Unexpected UI error: {:?}", app.error);
}

#[test]
fn showing_precise_advanced_eq_controls_does_not_round_the_stored_parameters() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::mastered(&ctx);
    app.tab = 1;
    app.experience.preferences.advanced = true;
    app.settings.manual_eq[3].freq = 873.623456;
    app.settings.manual_eq[3].gain = -1.234567;
    app.settings.manual_eq[3].q = 0.707123;
    app.settings.eq_output_db = -0.123456;
    let original = serde_json::to_value(&app.settings).unwrap();
    settled(&ctx, &mut app, egui::vec2(1480., 940.));
    assert_eq!(serde_json::to_value(&app.settings).unwrap(), original);
}

#[test]
fn a_changed_mastering_direction_cannot_start_a_stale_export_handler() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::mastered(&ctx);
    let folder = tempfile::tempdir().unwrap();
    app.settings.out_dir = folder.path().join("exports");
    settled(&ctx, &mut app, egui::vec2(1480., 940.));
    app.settings.intensity = 0.2;
    app.export_revision(&ctx);
    assert!(!app.busy);
    assert!(
        app.error
            .as_deref()
            .is_some_and(|text| text.contains("direction has changed"))
    );
    assert!(!app.settings.out_dir.exists());
}

#[test]
fn compact_processing_keeps_cancel_and_help_separate_and_reachable() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::app(&ctx);
    app.tab = 0;
    app.busy = true;
    app.experience.tour_paused = true;
    let viewport = egui::vec2(1100., 720.);
    let output = settled(&ctx, &mut app, viewport);
    let cancel = text_rect(&output, "Cancel").expect("Processing must expose Cancel");
    let help = text_rect(&output, "Help").expect("Help must remain visible");
    assert!(!cancel.intersects(help));
    click(&ctx, &mut app, viewport, &output, "Cancel");
    assert!(app.cancel.load(std::sync::atomic::Ordering::Relaxed));
}
