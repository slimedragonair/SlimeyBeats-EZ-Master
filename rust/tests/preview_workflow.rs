use mastercraft::{audio::Audio, config::Settings, dsp::manual_eq, engine, preview, profiles};

#[test]
fn shared_preview_matches_existing_chain_and_eq_revisions_do_not_accumulate() {
    let rate = 16000;
    let samples = (0..rate * 3)
        .flat_map(|i| {
            let t = i as f64 / rate as f64;
            let v = (0.12 * (t * 440. * std::f64::consts::TAU).sin()
                + 0.04 * (t * 1700. * std::f64::consts::TAU).sin()) as f32;
            [v, v * 0.85]
        })
        .collect();
    let input = Audio::new(samples, rate, 2).unwrap();
    let settings = Settings::default();
    let profile = profiles::resolve("neutral", std::path::Path::new("")).unwrap();
    let original = engine::master(
        &input,
        &settings,
        &profile,
        None,
        "test",
        &|_, _| {},
        &|| false,
    )
    .unwrap();
    let expected = manual_eq::render(&original.audio, &settings, &|| false).unwrap();
    let prepared = preview::master(
        &input,
        &settings,
        &profile,
        None,
        "test",
        &|_, _| {},
        &|| false,
    )
    .unwrap();
    assert_eq!(expected.audio.samples, prepared.audio.samples);
    let mut edited = settings.clone();
    edited.manual_eq[2].gain = -2.;
    edited.profile = "edm".into();
    let (first, _, report) =
        preview::resolve_eq(&prepared.raw, &edited, &profile, &prepared.report, &|| {
            false
        })
        .unwrap();
    let (again, _, _) =
        preview::resolve_eq(&prepared.raw, &edited, &profile, &report, &|| false).unwrap();
    assert_eq!(first.samples, again.samples);
    assert_eq!(report.configuration["profile"], "neutral");
    assert!(preview::resolve_eq(&prepared.raw, &edited, &profile, &report, &|| true).is_err());
}

#[test]
fn cancellation_immediately_before_export_commit_leaves_no_final_file() {
    let dir = tempfile::tempdir().unwrap();
    let audio = Audio::new(vec![0.05; 16000], 16000, 1).unwrap();
    let settings = Settings {
        out_dir: dir.path().into(),
        formats: vec!["wav24".into()],
        ..Default::default()
    };
    let calls = std::cell::Cell::new(0);
    let cancel = || {
        calls.set(calls.get() + 1);
        calls.get() >= 2
    };
    assert!(
        mastercraft::export::export(
            &audio,
            std::path::Path::new("original.wav"),
            &settings,
            &cancel
        )
        .is_err()
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
