use super::{
    test_fixture, theme,
    ui_test_input::*,
    worker::{Event, SuggestedEq},
};
use eframe::egui;
use std::sync::{Arc, atomic::Ordering};

fn queued(app: &super::app::MasterCraftApp) -> SuggestedEq {
    let (_, current) = app.current().unwrap();
    let mut bands = app.settings.manual_eq.clone();
    bands[6].gain = -1.25;
    SuggestedEq {
        source: current.raw.as_ref().unwrap_or(&current.before).clone(),
        profile: app.settings.profile.clone(),
        bands,
        note: "Gentle upper-treble cut. This is a heuristic; compare by ear.".into(),
    }
}

#[test]
fn queued_proposal_preserves_pending_eq_until_the_real_apply_button_is_clicked() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::mastered(&ctx);
    app.poll();
    app.settings.manual_eq[2].gain = 2.75;
    let before = app.settings.manual_eq.clone();
    let proposal = queued(&app);
    let expected = proposal.bands.clone();
    app.tx.send(Event::Suggestion(0, proposal)).unwrap();
    app.poll();
    assert_eq!(app.settings.manual_eq, before);
    assert_eq!(app.suggested_eq.as_ref(), Some(&expected));
    let viewport = egui::vec2(1100., 720.);
    let output = settled(&ctx, &mut app, viewport);
    assert_eq!(app.settings.manual_eq, before);
    assert!(output.shapes.iter().any(|item| matches!(&item.shape,
        egui::Shape::Text(text) if text.galley.text().contains("Gentle upper-treble cut.")
            && text.galley.text().contains("Not applied."))));
    click(&ctx, &mut app, viewport, &output, "Apply curve");
    assert_eq!(app.settings.manual_eq, expected);
    assert!(!app.player.controls.playing.load(Ordering::Relaxed));
    assert!(
        app.current()
            .unwrap()
            .1
            .report
            .as_ref()
            .unwrap()
            .exports
            .is_empty()
    );
}

#[test]
fn cancellation_replacement_audio_and_profile_changes_reject_queued_proposals() {
    for change in ["cancel", "source", "profile", "selection"] {
        let ctx = egui::Context::default();
        let mut app = test_fixture::mastered(&ctx);
        app.poll();
        app.settings.manual_eq[2].gain = 2.75;
        let before = app.settings.manual_eq.clone();
        let proposal = queued(&app);
        match change {
            "cancel" => app.cancel.store(true, Ordering::Relaxed),
            "profile" => app.settings.profile = "metal".into(),
            "selection" => app.selected = None,
            _ => {
                let mut replacement = app.current().unwrap().1.as_ref().clone();
                replacement.raw = Some(Arc::new(proposal.source.as_ref().clone()));
                app.tracks[0].result = Some(Arc::new(replacement));
            }
        }
        app.tx.send(Event::Suggestion(0, proposal)).unwrap();
        app.poll();
        assert!(app.suggested_eq.is_none(), "{change}");
        assert_eq!(app.settings.manual_eq, before, "{change}");
    }
}
