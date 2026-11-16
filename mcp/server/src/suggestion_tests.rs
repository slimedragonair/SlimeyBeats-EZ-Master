use super::*;
use crate::{arguments::Arguments, state::Engine};
use mastercraft::{audio::Audio, source_identity::SourceIdentity};
use std::{f64::consts::TAU, sync::atomic::AtomicBool};

fn audio(high_gain: f64) -> Arc<Audio> {
    Arc::new(
        Audio::new(
            (0..60000)
                .map(|frame| {
                    (0.02 * (TAU * 1000. * frame as f64 / 48000.).sin()
                        + high_gain * (TAU * 14000. * frame as f64 / 48000.).sin())
                        as f32
                })
                .collect(),
            48000,
            1,
        )
        .unwrap(),
    )
}

fn context(root: &Path) -> ContextData {
    let engine = Engine::new(&Arguments {
        input_root: vec![root.into()],
        output_root: root.join("exports"),
        max_decoded_mib: 8,
        decode_timeout_seconds: 5,
        desktop_exe: None,
        profiles_root: None,
        studio_data: Some(root.join("unused-runtime")),
        plugin_root: vec![],
        print_config: false,
    })
    .unwrap();
    let mut snapshot = engine.lock().unwrap().session.clone();
    snapshot.source = Some(Arc::new(Input {
        id: "current-source".into(),
        source: SessionSource {
            path: root.join("source.wav"),
            identity: SourceIdentity {
                bytes: 0,
                sha256: String::new(),
            },
        },
        audio: audio(0.001),
        generated: None,
        studio_preview: None,
    }));
    ContextData {
        engine,
        id: "suggestion-test".into(),
        snapshot,
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

#[test]
fn detailed_suggestion_retains_legacy_bands_and_leaves_the_session_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let ctx = context(root.path());
    let input = ctx.snapshot.source.as_ref().unwrap().audio.clone();
    let samples = input.samples.clone();
    let engine = ctx.engine.clone();
    let before = engine.session().unwrap();
    let measured = analysis::analyze(&input, &mastercraft::profiles::builtin()["neutral"]).unwrap();
    let expected = mastercraft::dsp::eq_suggestion::proposal(
        &measured,
        &mastercraft::profiles::builtin()["neutral"],
    )
    .unwrap();
    let product = analyze(ctx, "current-source".into(), true).unwrap();
    assert_eq!(product.result["bands"], json!(expected.bands));
    assert_eq!(product.result["notes"], json!(expected.notes));
    assert_eq!(
        product.result["high_frequency"],
        json!(expected.high_frequency)
    );
    assert_eq!(product.result["basis"], "original");
    assert_eq!(product.result["applied"], false);
    assert!(matches!(product.change, Change::None));
    assert_eq!(engine.session().unwrap(), before);
    assert_eq!(input.samples, samples);
    assert_eq!(
        analysis_summary(&measured)["high_frequency"],
        json!(measured.spectrum.high_frequency)
    );
}

#[test]
fn detailed_suggestion_uses_cached_pre_eq_master_without_publishing_a_revision() {
    let root = tempfile::tempdir().unwrap();
    let mut ctx = context(root.path());
    let raw = audio(0.08);
    let profile = mastercraft::profiles::builtin()["neutral"].clone();
    let mut settings = ctx.snapshot.settings.clone();
    settings
        .disabled
        .extend(["static_eq".into(), "dynamic_eq".into()]);
    let prepared = preview::master(&raw, &settings, &profile, None, "test", &|_, _| {}, &|| {
        false
    })
    .unwrap();
    let expected =
        mastercraft::dsp::eq_suggestion::proposal(&prepared.raw_analysis, &profile).unwrap();
    let raw_before = prepared.raw.samples.clone();
    let raw_pointer = prepared.raw.clone();
    let file = tempfile::NamedTempFile::new_in(root.path())
        .unwrap()
        .into_temp_path();
    ctx.snapshot.master = Some(Arc::new(Master {
        id: "current-preview".into(),
        preview: prepared,
        settings,
        file: Arc::new(file),
    }));
    let product = analyze(ctx, "current-source".into(), true).unwrap();
    assert_eq!(product.result["basis"], "master_before_manual_eq");
    assert_eq!(product.result["bands"], json!(expected.bands));
    assert_eq!(product.result["notes"], json!(expected.notes));
    assert_eq!(
        product.result["high_frequency"],
        json!(expected.high_frequency)
    );
    assert_eq!(raw_pointer.samples, raw_before);
    assert!(matches!(product.change, Change::None));
}
