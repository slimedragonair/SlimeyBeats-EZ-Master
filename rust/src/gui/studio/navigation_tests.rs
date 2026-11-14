//! Completed analysis reveals its result once; page navigation retains independent scroll.
use super::super::{
    Studio,
    interaction_tests::{fixture, frame, text},
    state::Page,
};
use super::{activate_compact, visible};
use crate::gui::theme;
use eframe::egui;
use std::time::{Duration, Instant};

fn settled(ctx: &egui::Context, studio: &mut Studio) -> egui::FullOutput {
    let mut output = frame(ctx, studio, false, egui::vec2(1100., 720.), 332., vec![]).0;
    for _ in 0..6 {
        output = frame(ctx, studio, false, egui::vec2(1100., 720.), 332., vec![]).0;
    }
    output
}

fn finish_job(ctx: &egui::Context, studio: &mut Studio) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while studio.busy() && Instant::now() < deadline {
        studio.poll(ctx);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!studio.busy(), "Headless Studio job did not finish");
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    ctx.all_styles_mut(|style| {
        style.animation_time = 0.;
        style.scroll_animation = egui::style::ScrollAnimation::none();
    });
    ctx
}

#[test]
fn completed_compact_spectrogram_reveals_whole_result_and_keeps_actions_visible() {
    let ctx = context();
    let mut studio = fixture(4);
    studio.page = Page::Repair;
    studio.mix.tracks[0].gain_db = -3.12506789;
    let mix = serde_json::to_value(&studio.mix).unwrap();
    let voice = serde_json::to_value(&studio.voice).unwrap();
    let repair = serde_json::to_value(&studio.repair).unwrap();
    settled(&ctx, &mut studio);
    studio.build_spectrogram(&ctx);
    finish_job(&ctx, &mut studio);
    assert!(studio.error.is_none(), "{:?}", studio.error);
    assert!(studio.spectrogram.is_some());
    let output = settled(&ctx, &mut studio);
    assert_eq!(
        visible(&output, "Frequency / Hz").len(),
        1,
        "Completed spectrogram header is below the viewport"
    );
    let texture = studio.texture.as_ref().unwrap().id();
    let plot = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                Some((shape.clip_rect, mesh.calc_bounds()))
            }
            _ => None,
        })
        .expect("Spectrogram texture mesh was not drawn");
    assert!(
        plot.0.contains_rect(plot.1),
        "Graph clipped: {:?} outside {:?}",
        plot.1,
        plot.0
    );
    let selection = text(&output, "Selection:").expect("Selection summary hidden");
    assert!(
        selection
            .clip_rect
            .contains_rect(selection.shape.visual_bounding_rect())
    );
    for label in ["Play", "Preview spectral repair", "Export current preview"] {
        assert_eq!(visible(&output, label).len(), 1, "Hidden {label}");
    }
    assert_eq!(serde_json::to_value(&studio.mix).unwrap(), mix);
    assert_eq!(serde_json::to_value(&studio.voice).unwrap(), voice);
    assert_eq!(serde_json::to_value(&studio.repair).unwrap(), repair);
    assert_eq!(studio.volume, 0.);
    assert!(!studio.reveal_spectrogram);
    // The completed-result reveal must not fight a later manual return to the fields.
    frame(
        &ctx,
        &mut studio,
        false,
        egui::vec2(1100., 720.),
        332.,
        vec![
            egui::Event::PointerMoved(egui::pos2(350., 390.)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0., 1000.),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    let output = settled(&ctx, &mut studio);
    assert_eq!(
        visible(&output, "Process this source").len(),
        1,
        "Reveal repeated after manual scrolling"
    );
}

#[test]
fn failed_cancelled_and_source_reset_jobs_do_not_reveal_a_previous_graph() {
    use super::super::{jobs::Change, state::LoadedSource};
    use std::sync::{Arc, atomic::Ordering};
    let ctx = context();
    let mut studio = fixture(4);
    studio.build_spectrogram(&ctx);
    finish_job(&ctx, &mut studio);
    assert!(studio.reveal_spectrogram);
    let previous = studio.spectrogram.clone().unwrap();
    studio.begin(&ctx, "Failed display preparation", |_, _| {
        Err(anyhow::anyhow!("fixture failure"))
    });
    finish_job(&ctx, &mut studio);
    assert!(studio.error.is_some());
    assert!(!studio.reveal_spectrogram);
    assert!(Arc::ptr_eq(&previous, studio.spectrogram.as_ref().unwrap()));
    let cancelled = previous.clone();
    studio.begin(&ctx, "Cancelled display preparation", move |cancel, _| {
        while !cancel.load(Ordering::Relaxed) {
            std::thread::yield_now();
        }
        Ok(Change::Spectrogram(cancelled))
    });
    studio.cancel();
    finish_job(&ctx, &mut studio);
    assert!(studio.error.as_deref().unwrap().contains("Cancelled"));
    assert!(!studio.reveal_spectrogram);
    assert!(Arc::ptr_eq(&previous, studio.spectrogram.as_ref().unwrap()));
    studio.build_spectrogram(&ctx);
    finish_job(&ctx, &mut studio);
    assert!(studio.reveal_spectrogram);
    let source = studio.source.as_ref().unwrap();
    let replacement = LoadedSource {
        audio: source.audio.clone(),
        path: source.path.clone(),
        loudness: source.loudness,
    };
    studio.begin(&ctx, "Replace source", move |_, _| {
        Ok(Change::Source(replacement))
    });
    finish_job(&ctx, &mut studio);
    assert!(studio.error.is_none());
    assert!(!studio.reveal_spectrogram);
    assert!(studio.spectrogram.is_none());
    assert_eq!(studio.volume, 0.);
}

#[test]
fn scrolling_repair_does_not_hide_stem_headers_or_share_other_page_offsets() {
    let ctx = context();
    let mut studio = fixture(4);
    studio.page = Page::Repair;
    let mix = serde_json::to_value(&studio.mix).unwrap();
    studio.build_spectrogram(&ctx);
    finish_job(&ctx, &mut studio);
    settled(&ctx, &mut studio);
    frame(
        &ctx,
        &mut studio,
        false,
        egui::vec2(1100., 720.),
        332.,
        vec![
            egui::Event::PointerMoved(egui::pos2(350., 390.)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0., -1000.),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    let output = settled(&ctx, &mut studio);
    let output = activate_compact(&ctx, &mut studio, &output, "Stems");
    assert_eq!(
        visible(&output, "Show part waveforms").len(),
        1,
        "Repair scroll leaked into Stems"
    );
    assert!(text(&output, "4 parts ·").is_some());
    assert_eq!(visible(&output, "Mute").len(), 4);
    assert_eq!(visible(&output, "Solo").len(), 4);
    let output = activate_compact(&ctx, &mut studio, &output, "Voice");
    assert_eq!(visible(&output, "Process this source").len(), 1);
    assert_eq!(visible(&output, "Noise reduction").len(), 1);
    let output = activate_compact(&ctx, &mut studio, &output, "Repair");
    assert!(
        text(&output, "Selection:").is_some(),
        "Repair should retain its own scroll position"
    );
    assert_eq!(serde_json::to_value(&studio.mix).unwrap(), mix);
    assert_eq!(studio.volume, 0.);
}

#[test]
fn plugin_input_and_choosers_are_visible_once_in_the_workspace_in_both_modes_and_sizes() {
    for size in [egui::vec2(1100., 720.), egui::vec2(1480., 940.)] {
        for advanced in [false, true] {
            let ctx = context();
            let mut studio = fixture(4);
            studio.page = Page::Plugins;
            studio.plugin_host = Some(crate::studio_native::plugins::VstHost {
                executable: "not-executed-fixture-host".into(),
            });
            studio.plugin_class = "fixture".into();
            studio.plugin_probe = Some(crate::studio_native::plugins::Probe {
                plugins: vec![crate::studio_native::plugins::PluginInfo {
                    class: crate::studio_native::plugins::PluginClass {
                        id: "fixture".into(),
                        name: "Fixture effect".into(),
                        category: "AudioFx".into(),
                    },
                    parameters: vec![],
                    error: None,
                }],
            });
            let mix = serde_json::to_value(&studio.mix).unwrap();
            let mut output = None;
            for _ in 0..6 {
                output = Some(frame(&ctx, &mut studio, advanced, size, 332., vec![]).0);
            }
            let output = output.as_ref().unwrap();
            for label in [
                "Process this source",
                "Choose VST3 file…",
                "Choose bundle folder…",
            ] {
                let controls = visible(output, label);
                assert_eq!(
                    controls.len(),
                    1,
                    "Plugin control hidden or duplicated: {label}"
                );
                assert!(
                    controls[0].shape.visual_bounding_rect().right() < size.x - 332.,
                    "Plugin control still in sidebar: {label}"
                );
            }
            for label in ["Preview plugin effect", "Export current preview"] {
                assert_eq!(visible(output, label).len(), 1);
            }
            assert!(text(output, "not a security sandbox").is_some());
            assert!(text(output, "Mono/stereo float32").is_some());
            assert_eq!(serde_json::to_value(&studio.mix).unwrap(), mix);
            assert!(
                !studio.busy(),
                "Rendering plugin choices must not execute the helper"
            );
            assert_eq!(studio.volume, 0.);
        }
    }
}
