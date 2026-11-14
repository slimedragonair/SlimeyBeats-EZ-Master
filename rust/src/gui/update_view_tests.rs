use super::{test_fixture, theme, ui_test_input::*, user_experience::HelpPage};
use crate::app_update::{self, VerifiedInstaller};
use eframe::egui;

#[test]
fn committed_setup_handoff_blocks_new_audio_input() {
    #[derive(Debug)]
    struct Dropped(std::path::PathBuf);
    impl egui::DroppedFile for Dropped {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
    }
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::mastered(&ctx);
    settled(&ctx, &mut app, egui::vec2(1100., 720.));
    let before = app.tracks.len();
    app.updates.opening_setup = true;
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1100., 720.),
        )),
        dropped_files: vec![std::sync::Arc::new(Dropped("new-unsaved-track.wav".into()))],
        ..Default::default()
    };
    let mut output = ctx.run_ui(raw, |ui| app.render_frame(ui));
    output.textures_delta.clear();
    assert!(text_rect(&output, "Verifying and opening Setup…").is_some());
    assert_eq!(app.tracks.len(), before);
    assert!(!app.busy && !app.studio.busy());
}

#[test]
fn update_indicator_opens_updates_and_restores_keyboard_focus_without_interrupting_audio() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::mastered(&ctx);
    let size = egui::vec2(1100., 720.);
    app.updates.available = Some(app_update::fixture_candidate(
        app_update::identity::BUILD_REVISION + 1,
        &"a".repeat(64),
    ));
    let output = settled(&ctx, &mut app, size);
    assert!(text_rect(&output, "Update available").is_some());
    let output = click(&ctx, &mut app, size, &output, "Update available");
    assert_eq!(app.experience.help_page, Some(HelpPage::Updates));
    assert!(
        text_rect(&output, "Download update").is_some(),
        "The primary download must precede collapsed release notes"
    );
    click(&ctx, &mut app, size, &output, "Close help");
    let key = |pressed| egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&ctx, &mut app, size, vec![key(true)]);
    frame(&ctx, &mut app, size, vec![key(false)]);
    assert_eq!(app.experience.help_page, Some(HelpPage::Updates));
    assert!(app.current().is_some());
}

#[test]
fn updates_help_exposes_real_actions_and_preserves_audio_settings() {
    for size in [egui::vec2(1100., 720.), egui::vec2(1480., 940.)] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = test_fixture::mastered(&ctx);
        let settings = serde_json::to_vec(&app.settings).unwrap();
        app.experience.open_help(HelpPage::Updates, &ctx);
        let output = settled(&ctx, &mut app, size);
        for label in [
            "Updates & tools",
            "Check for updates",
            "Check for updates automatically",
            "Close help",
        ] {
            assert!(
                text_rect(&output, label).is_some(),
                "Missing or clipped {label} at {size:?}"
            );
        }
        assert!(!app.updates.busy());
        assert_eq!(serde_json::to_vec(&app.settings).unwrap(), settings);
    }
}
#[test]
fn install_with_loaded_audio_requires_explicit_save_confirmation_and_can_be_cancelled() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::mastered(&ctx);
    let size = egui::vec2(1480., 940.);
    app.updates.downloaded = Some(VerifiedInstaller {
        path: "never-run.exe".into(),
        identity: app_update::current(),
        platform: app_update::WINDOWS_X86_64.into(),
        installer_sha256: "a".repeat(64),
        installer_bytes: 3,
        executable_sha256: "b".repeat(64),
        release_url: app_update::MANUAL_RELEASE_URL.into(),
    });
    app.experience.open_help(HelpPage::Updates, &ctx);
    let output = settled(&ctx, &mut app, size);
    let output = click(&ctx, &mut app, size, &output, "Install update…");
    assert!(app.updates.confirm_install);
    assert!(!app.updates.busy() && !app.updates.close_after_launch);
    assert!(text_rect(&output, "Work saved · open Setup").is_some());
    click(&ctx, &mut app, size, &output, "Go back and save");
    assert!(!app.updates.confirm_install && !app.updates.close_after_launch);
    assert!(app.current().is_some());
}
