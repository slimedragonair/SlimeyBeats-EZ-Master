//! Shared in-memory GUI fixture. No native device or real preference file is opened.
use super::{app::MasterCraftApp, brand_art::BrandArt};
use crate::config::Settings;
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool, mpsc},
};

pub(super) fn app(ctx: &egui::Context) -> MasterCraftApp {
    let (tx, rx) = mpsc::channel();
    let experience = crate::gui::user_experience::UserExperience::for_tests();
    let mut studio = crate::gui::studio::Studio::new(PathBuf::new());
    studio.player = crate::gui::player::Player::for_processing_tests();
    studio.runtime = crate::studio_native::runtime::RuntimePaths {
        base: studio.cache.as_ref().unwrap().path().join("test-runtime"),
    };
    MasterCraftApp {
        experience,
        preferences: None,
        studio,
        brand: BrandArt::new(ctx),
        settings: Settings::default(),
        root: PathBuf::new(),
        tracks: vec![],
        selected: None,
        busy: false,
        status: String::new(),
        progress: 0.,
        error: None,
        eq_dirty: false,
        eq_live: Default::default(),
        tab: 1,
        selected_band: 3,
        tx,
        rx,
        active_job: None,
        cancel: Arc::new(AtomicBool::new(false)),
        player: crate::gui::player::Player::for_processing_tests(),
        waveform: Default::default(),
        eq_waveform: Default::default(),
        playback_spectrum: Default::default(),
        suggested_eq: None,
        suggestion_note: None,
        loaded: None,
        loaded_eq: false,
        volume: 0.5,
        match_levels: true,
        capture: None,
        capture_settings_scroll: None,
        capture_workspace_scroll: None,
    }
}

pub(super) fn mastered(ctx: &egui::Context) -> MasterCraftApp {
    let mut app = app(ctx);
    let before = Arc::new(
        crate::audio::Audio::new(
            (0..24000)
                .map(|frame| (frame as f32 * 0.06).sin() * 0.08)
                .collect(),
            48000,
            1,
        )
        .unwrap(),
    );
    let profile = crate::profiles::resolve("neutral", std::path::Path::new(".")).unwrap();
    let preview = crate::preview::master(
        &before,
        &app.settings,
        &profile,
        None,
        "fixture.wav",
        &|_, _| {},
        &|| false,
    )
    .unwrap();
    let result = crate::gui::worker::TrackResult {
        before,
        raw: Some(preview.raw),
        raw_analysis: Some(preview.raw_analysis),
        eq_parameters: Some(preview.parameters),
        after: Some(preview.audio),
        analysis: preview.report.input.clone(),
        report: Some(preview.report),
    };
    app.tracks.push(crate::gui::app::Track {
        path: "fixture.wav".into(),
        status: "Preparing".into(),
        result: None,
    });
    app.selected = Some(0);
    app.busy = true;
    app.active_job = Some(crate::gui::app::JobKind::Master);
    app.tx
        .send(crate::gui::worker::Event::Ready(0, Arc::new(result)))
        .unwrap();
    app.tx.send(crate::gui::worker::Event::Finished).unwrap();
    app
}
