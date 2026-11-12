use super::*;
use crate::gui::{app::Track, brand_art::BrandArt, worker::TrackResult};
use std::sync::{Arc, atomic::AtomicBool, mpsc};

fn fixture(ctx: &egui::Context) -> MasterCraftApp {
    let (tx, rx) = mpsc::channel();
    MasterCraftApp {
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
        player: Default::default(),
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

fn load_preview(app: &mut MasterCraftApp) {
    let audio = Arc::new(crate::audio::Audio::new(vec![0.; 96], 48000, 2).unwrap());
    let analysis = crate::analysis::Analysis {
        duration_s: 48.,
        sample_rate: 48000,
        channels: 2,
        metrics: Default::default(),
        spectrum: crate::analysis::Spectrum {
            freqs: vec![],
            db: vec![],
            cutoff_hz: None,
            quiet_hf_flatness: 0.,
            noise_candidate: false,
            sibilance_spike_fraction: 0.,
            octave_power: vec![],
        },
        measured_db: vec![],
        deviation_db: vec![],
        valid: vec![],
        issues: vec![],
        suggested_profile: "neutral".into(),
    };
    app.tracks.push(Track {
        path: "test_song.wav".into(),
        status: "Preview ready".into(),
        result: Some(Arc::new(TrackResult {
            before: audio.clone(),
            raw: None,
            raw_analysis: None,
            eq_parameters: None,
            after: Some(audio),
            analysis,
            report: None,
        })),
    });
    app.selected = Some(0);
}

fn panel_width(
    ctx: &egui::Context,
    app: &mut MasterCraftApp,
    width: f32,
    viewport: egui::Vec2,
    render: bool,
) -> f32 {
    let mut actual = 0.;
    for _ in 0..64 {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
                ..Default::default()
            },
            |ui| {
                actual = egui::Panel::right("settings-width-regression")
                    .default_size(width)
                    .min_size(290.)
                    .max_size(450.)
                    .resizable(true)
                    .show(ui, |ui| {
                        egui::Frame::new().inner_margin(12).show(ui, |ui| {
                            if render {
                                show(app, ui, ctx);
                            } else {
                                ui.set_min_width(ui.available_width());
                            }
                        });
                    })
                    .response
                    .rect
                    .width();
            },
        );
        output.textures_delta.clear();
    }
    actual
}

#[test]
fn expanded_settings_and_loaded_preview_do_not_expand_resizable_panel() {
    for (width, viewport, scale) in [
        (332., egui::vec2(1480., 940.), 1.),
        (290., egui::vec2(1100., 720.), 1.),
        (450., egui::vec2(1480., 940.), 1.),
        (332., egui::vec2(1480., 940.), 1.25),
        (290., egui::vec2(1100., 720.), 1.25),
    ] {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(scale);
        theme::apply(&ctx);
        let mut app = fixture(&ctx);
        let chosen_width = panel_width(&ctx, &mut app, width, viewport, false);
        let welcome_width = panel_width(&ctx, &mut app, width, viewport, true);
        load_preview(&mut app);
        app.settings.profile = format!("custom:{}", "Long mastering profile name ".repeat(8));
        app.settings.out_dir = PathBuf::from(format!(
            "F:/Audio/{}/Exports",
            "Long project directory/".repeat(12)
        ));
        app.settings.target_lufs = Some(-14.123456);
        app.settings.ceiling_dbtp = -1.123456;
        app.settings.match_strength = 1.123456;
        app.settings.max_limiter_gr_db = 6.123456;
        app.settings.max_makeup_db = 18.123456;
        app.settings.output_sr = 192000;
        ctx.memory_mut(|memory| memory.set_everything_is_visible(true));
        for reference in [
            None,
            Some(PathBuf::from(format!(
                "F:/Audio/{}.wav",
                "Reference title ".repeat(16)
            ))),
        ] {
            app.settings.reference = reference;
            let before = serde_json::to_value(&app.settings).unwrap();
            let loaded_width = panel_width(&ctx, &mut app, width, viewport, true);
            assert!(
                welcome_width <= chosen_width + 0.5,
                "welcome: {welcome_width}, chosen: {chosen_width}"
            );
            assert!(
                loaded_width <= chosen_width + 0.5,
                "loaded: {loaded_width}, chosen: {chosen_width}"
            );
            assert_eq!(serde_json::to_value(&app.settings).unwrap(), before);
        }
    }
}

fn ui_frame(
    ctx: &egui::Context,
    app: &mut MasterCraftApp,
    height: f32,
    events: Vec<egui::Event>,
) -> (egui::FullOutput, egui::scroll_area::State, f32) {
    let mut scroll_id = None;
    let mut inner_bottom = 0.;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(332., height),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            egui::Frame::new().inner_margin(12).show(ui, |ui| {
                inner_bottom = ui.max_rect().bottom();
                scroll_id = Some(ui.make_persistent_id(egui::IdSalt::new("settings-scroll")));
                show(app, ui, ctx);
            });
        },
    );
    output.textures_delta.clear();
    let scroll = egui::scroll_area::State::load(ctx, scroll_id.unwrap()).unwrap();
    (output, scroll, inner_bottom)
}

fn text_shape<'a>(
    output: &'a egui::FullOutput,
    label: &str,
) -> Option<&'a egui::epaint::ClippedShape> {
    output.shapes.iter().find(|clipped| match &clipped.shape {
        egui::Shape::Text(text) => {
            text.galley.text() == label
                && clipped
                    .clip_rect
                    .intersects(clipped.shape.visual_bounding_rect())
        }
        _ => false,
    })
}

fn click_text(
    ctx: &egui::Context,
    app: &mut MasterCraftApp,
    output: &egui::FullOutput,
    label: &str,
) -> egui::FullOutput {
    let pos = text_shape(output, label)
        .unwrap()
        .shape
        .visual_bounding_rect()
        .center();
    let pointer = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    ui_frame(
        ctx,
        app,
        1400.,
        vec![egui::Event::PointerMoved(pos), pointer(true)],
    );
    ui_frame(ctx, app, 1400., vec![pointer(false)]).0
}

#[test]
fn export_disclosure_starts_closed_and_remembers_user_toggles() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    ctx.all_styles_mut(|style| style.animation_time = 0.);
    let mut app = fixture(&ctx);
    let before = serde_json::to_value(&app.settings).unwrap();
    let output = ui_frame(&ctx, &mut app, 1400., vec![]).0;
    assert!(text_shape(&output, "Export settings").is_some());
    assert!(text_shape(&output, "Formats: WAV 24-bit").is_some());
    assert!(text_shape(&output, "WAV 24-bit").is_none());
    assert!(text_shape(&output, "File name suffix").is_none());
    click_text(&ctx, &mut app, &output, "Export settings");
    let output = ui_frame(&ctx, &mut app, 1400., vec![]).0;
    assert!(text_shape(&output, "WAV 24-bit").is_some());
    assert!(text_shape(&output, "File name suffix").is_some());
    let output = click_text(&ctx, &mut app, &output, "FLAC 24-bit");
    assert!(app.settings.formats.iter().any(|format| format == "flac24"));
    let output = click_text(&ctx, &mut app, &output, "FLAC 24-bit");
    assert!(!app.settings.formats.iter().any(|format| format == "flac24"));
    click_text(&ctx, &mut app, &output, "Export settings");
    let output = ui_frame(&ctx, &mut app, 1400., vec![]).0;
    assert!(text_shape(&output, "Formats: WAV 24-bit").is_some());
    assert!(text_shape(&output, "WAV 24-bit").is_none());
    assert_eq!(serde_json::to_value(&app.settings).unwrap(), before);
    assert_eq!(app.capture_settings_scroll, None);
}

#[test]
fn capture_forces_only_export_open_without_changing_settings() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    ctx.all_styles_mut(|style| style.animation_time = 0.);
    let mut app = fixture(&ctx);
    app.capture_settings_scroll = Some(0.);
    let before = serde_json::to_value(&app.settings).unwrap();
    let output = ui_frame(&ctx, &mut app, 1400., vec![]).0;
    assert!(text_shape(&output, "WAV 24-bit").is_some());
    assert!(text_shape(&output, "File name suffix").is_some());
    assert!(text_shape(&output, "Example: metal:0.6+edm:0.4").is_none());
    assert!(text_shape(&output, "Limiter GR / dB").is_none());
    assert_eq!(serde_json::to_value(&app.settings).unwrap(), before);
    assert_eq!(app.capture_settings_scroll, Some(0.));
}

#[test]
fn capture_scroll_repeats_offset_and_insets_viewport_bottom() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    ctx.all_styles_mut(|style| style.animation_time = 0.);
    let mut app = fixture(&ctx);
    app.capture_settings_scroll = Some(300.);
    for _ in 0..4 {
        let (output, scroll, bottom) = ui_frame(&ctx, &mut app, 600., vec![]);
        assert!((scroll.clamped_offset().y - 300.).abs() < 0.5);
        let format =
            text_shape(&output, "WAV 24-bit").expect("capture scroll should reveal export formats");
        assert!(format.clip_rect.bottom() <= bottom - SCROLL_BOTTOM_INSET + 0.5);
        assert_eq!(app.capture_settings_scroll, Some(300.));
    }
}
