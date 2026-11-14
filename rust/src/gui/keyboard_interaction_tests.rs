//! Modified system shortcuts must not activate transport or a focused tutorial.
use super::{test_fixture, theme, ui_test_input::*};
use eframe::egui;
use std::sync::atomic::Ordering;

fn space(modifiers: egui::Modifiers, repeat: bool) -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed: true,
        repeat,
        modifiers,
    }
}

#[test]
fn transport_accepts_one_plain_space_without_stealing_modified_shortcuts() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::mastered(&ctx);
    app.tab = 0;
    let viewport = egui::vec2(1100., 720.);
    settled(&ctx, &mut app, viewport);
    for modifiers in [
        egui::Modifiers::ALT,
        egui::Modifiers::CTRL,
        egui::Modifiers::SHIFT,
        egui::Modifiers::COMMAND,
    ] {
        frame(&ctx, &mut app, viewport, vec![space(modifiers, false)]);
        assert!(
            !app.player.controls.playing.load(Ordering::Relaxed),
            "Modified Space started playback: {modifiers:?}"
        );
        frame(
            &ctx,
            &mut app,
            viewport,
            vec![egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            }],
        );
    }
    frame(
        &ctx,
        &mut app,
        viewport,
        vec![space(egui::Modifiers::NONE, false)],
    );
    assert!(app.player.controls.playing.load(Ordering::Relaxed));
    frame(
        &ctx,
        &mut app,
        viewport,
        vec![space(egui::Modifiers::NONE, true)],
    );
    assert!(
        app.player.controls.playing.load(Ordering::Relaxed),
        "A held key toggled playback repeatedly"
    );
    assert!(!app.busy && !app.studio.busy());
}

#[test]
fn alt_space_does_not_activate_the_focused_first_run_choice() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::app(&ctx);
    app.experience.welcome_open = true;
    app.experience.request_dialog_focus = true;
    let viewport = egui::vec2(1100., 720.);
    settled(&ctx, &mut app, viewport);
    frame(
        &ctx,
        &mut app,
        viewport,
        vec![space(egui::Modifiers::ALT, false)],
    );
    settled(&ctx, &mut app, viewport);
    assert!(
        app.experience.welcome_open && !app.experience.tour_open,
        "The OS Size-menu shortcut activated a tutorial choice"
    );
}
