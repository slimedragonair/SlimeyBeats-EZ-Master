use super::*;
use mastercraft::{config::Settings, dsp::eq_suggestion};
use serde_json::json;
use std::{f64::consts::TAU, io::Cursor};

fn tones(rate: u32, channels: u32, gains: [f64; 3]) -> Vec<f32> {
    (0..rate * 5 / 4)
        .flat_map(|frame| {
            let sample = [120., 1000., 8000.]
                .into_iter()
                .zip(gains)
                .map(|(frequency, gain)| {
                    gain * (TAU * frequency * frame as f64 / rate as f64).sin()
                })
                .sum::<f64>();
            (0..channels).map(move |channel| (sample * if channel == 0 { 1. } else { -0.5 }) as f32)
        })
        .collect()
}

fn fixture() -> (Session, Vec<f32>) {
    let input = tones(48000, 2, [0.08, 0.015, 0.005]);
    let raw = tones(48000, 2, [0.005, 0.02, 0.08]);
    (Session::new(input, 48000, 2).unwrap(), raw)
}

fn bits(samples: &[f32]) -> Vec<u32> {
    samples.iter().map(|sample| sample.to_bits()).collect()
}

fn state(session: &Session) -> Value {
    json!({
        "input": bits(&session.input.samples),
        "input_ptr": session.input.samples.as_ptr() as usize,
        "reference": session.reference,
        "raw": session.raw.as_ref().map(|raw| bits(&raw.samples)),
        "raw_ptr": session.raw.as_ref().map(|raw| raw.samples.as_ptr() as usize),
        "report": session.report,
        "raw_reference_stale": session.raw_reference_stale,
    })
}

fn numeric_projection(report: &Value) -> String {
    let mut projection = json!({});
    for key in [
        "target_lufs",
        "ceiling_dbtp",
        "processing_seconds",
        "punch_loss_db",
        "safety_attempts",
        "limiter",
    ] {
        projection[key] = report[key].clone();
    }
    projection.to_string()
}

fn decode(bytes: &[u8]) -> Audio {
    let mut reader = hound::WavReader::new(Cursor::new(bytes)).unwrap();
    let spec = reader.spec();
    Audio::new(
        reader.samples::<f32>().map(Result::unwrap).collect(),
        spec.sample_rate,
        spec.channels as usize,
    )
    .unwrap()
}

fn amplitude(audio: &Audio, frequency: f64, channel: usize) -> f64 {
    // Use complete settled tone periods, excluding startup and EOF transients.
    let start = audio.sample_rate as usize / 4;
    let end = audio.sample_rate as usize;
    let (mut real, mut imaginary) = (0., 0.);
    for frame in start..end {
        let phase = TAU * frequency * frame as f64 / audio.sample_rate as f64;
        let sample = audio.samples[frame * audio.channels + channel] as f64;
        real += sample * phase.cos();
        imaginary += sample * phase.sin();
    }
    2. * real.hypot(imaginary) / (end - start) as f64
}

#[test]
fn adopts_exact_pcm_with_input_layout_and_locally_measured_report() {
    for (rate, channels) in [(8000, 1), (44100, 2), (48000, 1), (48000, 2)] {
        let input = tones(rate, channels, [0.08, 0.015, 0.005]);
        let mut raw = tones(rate, channels, [0.005, 0.02, 0.08]);
        raw[0] = -0.;
        let mut session = Session::new(input.clone(), rate, channels).unwrap();
        let mut settings = Settings {
            profile: "edm".into(),
            ..Settings::default()
        };
        settings.manual_eq[3].gain = 9.;
        settings.eq_output_db = 6.;
        let serverreport = json!({
            "target_lufs": -16., "ceiling_dbtp": -2., "processing_seconds": 7.25,
            "punch_loss_db": 1.25, "safety_attempts": 2,
            "limiter": {"iterations": 3, "reachable": true, "pre_gain_db": 4.,
                "max_gr_db": 2.5, "absolute_max_gr_db": 3., "trim_db": -0.2,
                "out_lufs": -16.1, "out_tp_dbtp": -2.1},
            "file": "PRIVATE_SERVER_PATH", "version": "REMOTE_VERSION",
            "profile": "REMOTE_PROFILE", "configuration": {"out_dir": "PRIVATE_SERVER_PATH"},
            "notes": ["REMOTE_NOTE"], "log": [{"module": "REMOTE_MODULE"}],
            "exports": [{"path": "PRIVATE_SERVER_PATH"}],
            "input": {"metrics": {"integrated_lufs": 123}},
            "output": {"metrics": {"integrated_lufs": 123}},
        })
        .to_string();
        let response = session
            .adopt_master(
                raw.clone(),
                &serde_json::to_string(&settings).unwrap(),
                &serverreport,
            )
            .unwrap();
        let result: Value = serde_json::from_str(&response).unwrap();
        let profile = profile("edm").unwrap();
        let expected_input = analysis::analyze(
            &Audio::new(input.clone(), rate, channels as usize).unwrap(),
            &profile,
        )
        .unwrap();
        let expected_output = analysis::analyze(
            &Audio::new(raw.clone(), rate, channels as usize).unwrap(),
            &profile,
        )
        .unwrap();
        assert_eq!(result["report"]["input"], json!(expected_input));
        assert_eq!(result["report"]["output"], json!(expected_output));
        assert_eq!(result["raw_analysis"], result["report"]["output"]);
        assert_eq!(
            result["report"],
            serde_json::from_str::<Value>(&session.report().unwrap()).unwrap()
        );
        assert_eq!(result["report"]["profile"], profile["display_name"]);
        assert_eq!(result["report"]["version"], ENGINE_VERSION);
        assert_eq!(result["report"]["file"], "browser-track");
        assert_eq!(result["report"]["target_lufs"], -16.);
        assert_eq!(result["report"]["processing_seconds"], 7.25);
        assert_eq!(result["report"]["safety_attempts"], 2);
        assert_eq!(result["report"]["limiter"]["max_gr_db"], 2.5);
        assert_eq!(result["report"]["notes"], json!([SERVER_NOTE]));
        assert_eq!(result["report"]["log"], json!([]));
        assert_eq!(result["report"]["exports"], json!([]));
        settings.manual_eq = mastercraft::config::default_eq();
        settings.eq_output_db = 0.;
        assert_eq!(result["report"]["configuration"], json!(settings));
        for private in [
            "PRIVATE_SERVER_PATH",
            "REMOTE_VERSION",
            "REMOTE_PROFILE",
            "REMOTE_NOTE",
            "REMOTE_MODULE",
        ] {
            assert!(!response.contains(private));
        }
        let adopted = session.raw.as_ref().unwrap();
        assert_eq!(
            (adopted.sample_rate, adopted.channels),
            (rate, channels as usize)
        );
        assert_eq!(bits(&adopted.samples), bits(&raw));
        assert_eq!(session.preview_ptr().unwrap(), adopted.samples.as_ptr());
        assert_eq!(session.preview_len().unwrap(), raw.len());
        assert_eq!(bits(&session.input.samples), bits(&input));
    }
}

#[test]
fn invalid_pcm_settings_and_metadata_preserve_every_prior_field() {
    let (mut session, raw) = fixture();
    session
        .set_reference(tones(44100, 1, [0.01, 0.02, 0.03]), 44100, 1)
        .unwrap();
    session.adopt_master(raw.clone(), "{}", "{}").unwrap();
    session.export_wav("{}", "wav32f").unwrap();
    let before = state(&session);
    let mut cases: Vec<(Vec<f32>, String, String)> = vec![
        (vec![], "{}".into(), "{}".into()),
        (raw[..raw.len() - 1].to_vec(), "{}".into(), "{}".into()),
        (raw[..raw.len() - 2].to_vec(), "{}".into(), "{}".into()),
        (
            [raw.as_slice(), &[0., 0.]].concat(),
            "{}".into(),
            "{}".into(),
        ),
    ];
    for sample in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut samples = raw.clone();
        samples[42] = sample;
        cases.push((samples, "{}".into(), "{}".into()));
    }
    for settings in [
        "{",
        r#"{"profile":"/private/profile.json"}"#,
        r#"{"profile":"neutral:NaN+edm"}"#,
        r#"{"intensity":2}"#,
        r#"{"manual_eq":[]}"#,
        r#"{"eq_output_db":13}"#,
    ] {
        cases.push((raw.clone(), settings.into(), "{}".into()));
    }
    for metadata in [
        "{",
        "null",
        "[]",
        "1",
        r#"{"target_lufs":"-14"}"#,
        r#"{"ceiling_dbtp":true}"#,
        r#"{"processing_seconds":null}"#,
        r#"{"punch_loss_db":1e999}"#,
        r#"{"safety_attempts":1.5}"#,
        r#"{"safety_attempts":-1}"#,
        r#"{"limiter":[]}"#,
        r#"{"limiter":{"reachable":1}}"#,
        r#"{"limiter":{"iterations":false}}"#,
        r#"{"limiter":{"pre_gain_db":"private"}}"#,
    ] {
        cases.push((raw.clone(), "{}".into(), metadata.into()));
    }
    cases.push((raw.clone(), "{}".into(), " ".repeat(MAX_REPORT_BYTES + 1)));
    for (samples, settings, metadata) in cases {
        assert!(
            session
                .adopt_master_checked(samples, &settings, &metadata)
                .is_err()
        );
        assert_eq!(state(&session), before);
    }
}

#[test]
fn invalid_first_adoption_does_not_create_a_master() {
    let (mut session, raw) = fixture();
    let before = state(&session);
    assert!(
        session
            .adopt_master_checked(raw, "{}", r#"{"limiter":null}"#)
            .is_err()
    );
    assert_eq!(state(&session), before);
    assert!(session.raw.is_none());
    assert!(session.report.is_none());
}

#[test]
fn adoption_rechecks_browser_import_limits_before_exact_length() {
    let input = Audio::new(vec![0.; 16], 8000, 1).unwrap();
    assert!(
        validate_layout(&input, 7_200_001)
            .unwrap_err()
            .to_string()
            .contains("15 minutes")
    );
    assert!(
        validate_layout(
            &input,
            import_limits::MAX_DECODED_BYTES / size_of::<f32>() + 1
        )
        .unwrap_err()
        .to_string()
        .contains("384 MiB")
    );
    assert!(
        validate_layout(&input, 15)
            .unwrap_err()
            .to_string()
            .contains("exactly")
    );
    assert!(validate_layout(&input, 16).is_ok());
}

#[test]
fn projected_metadata_is_bounded_and_wrong_numeric_types_are_rejected() {
    let (mut session, raw) = fixture();
    let result: Value = serde_json::from_str(
        &session
            .adopt_master(
                raw,
                "{}",
                &json!({
                    "target_lufs": -1e100, "ceiling_dbtp": 1e100, "processing_seconds": 1e100,
                    "punch_loss_db": -1e100, "safety_attempts": u64::MAX,
                    "limiter": {"iterations": u64::MAX, "pre_gain_db": 1e100, "max_gr_db": -1e100,
                        "absolute_max_gr_db": 1e100, "trim_db": -1e100,
                        "out_lufs": -1e100, "out_tp_dbtp": 1e100}
                })
                .to_string(),
            )
            .unwrap(),
    )
    .unwrap();
    let report = &result["report"];
    assert_eq!(report["target_lufs"], -MAX_DB);
    assert_eq!(report["ceiling_dbtp"], 0.);
    assert_eq!(report["processing_seconds"], MAX_PROCESSING_SECONDS);
    assert_eq!(report["punch_loss_db"], 0.);
    assert_eq!(report["safety_attempts"], MAX_ATTEMPTS);
    assert_eq!(
        report["limiter"],
        json!({"iterations":MAX_ATTEMPTS,"reachable":false,
        "pre_gain_db":MAX_DB,"max_gr_db":0.,"absolute_max_gr_db":MAX_DB,
        "trim_db":-MAX_DB,"out_lufs":-MAX_DB,"out_tp_dbtp":MAX_DB})
    );
    for key in [
        "target_lufs",
        "ceiling_dbtp",
        "processing_seconds",
        "punch_loss_db",
    ] {
        assert!(number(&json!({key: true}), key, 0., -MAX_DB, MAX_DB).is_err());
    }
    for key in [
        "pre_gain_db",
        "max_gr_db",
        "absolute_max_gr_db",
        "trim_db",
        "out_lufs",
        "out_tp_dbtp",
    ] {
        assert!(
            limiter_metadata(
                Some(&json!({key: "0"})),
                &session.report.as_ref().unwrap().output
            )
            .is_err()
        );
    }
}

#[test]
fn numeric_metadata_and_exact_pcm_can_be_readopted_for_recovery() {
    let (mut session, raw) = fixture();
    let serverreport = json!({"target_lufs":-16.,"ceiling_dbtp":-2.,
        "processing_seconds":3.5,"punch_loss_db":0.75,"safety_attempts":2,
        "limiter":{"reachable":true,"iterations":4,"max_gr_db":1.5}})
    .to_string();
    let first: Value = serde_json::from_str(
        &session
            .adopt_master(raw.clone(), "{}", &serverreport)
            .unwrap(),
    )
    .unwrap();
    session
        .export_wav(r#"{"eq_output_db":-3,"eq_hold_loudness":false}"#, "wav32f")
        .unwrap();
    let saved_report: Value = serde_json::from_str(&session.report().unwrap()).unwrap();
    let saved_metadata = numeric_projection(&saved_report);
    let mut recovered = Session::new(session.input.samples.clone(), 48000, 2).unwrap();
    let restored: Value = serde_json::from_str(
        &recovered
            .adopt_master(raw.clone(), "{}", &saved_metadata)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first, restored);
    assert_eq!(bits(&recovered.raw.as_ref().unwrap().samples), bits(&raw));
    assert_eq!(
        session.export_wav("{}", "wav32f").unwrap(),
        recovered.export_wav("{}", "wav32f").unwrap()
    );
}

#[test]
fn suggestions_measure_adopted_master_instead_of_original() {
    let (mut session, raw) = fixture();
    let before = session.suggest("{}").unwrap();
    session.adopt_master(raw, "{}", "{}").unwrap();
    let profile = profile("neutral").unwrap();
    let measurement = analysis::analyze(session.raw.as_ref().unwrap(), &profile).unwrap();
    let expected = eq_suggestion::suggest(&measurement, &profile).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&session.suggest("{}").unwrap()).unwrap(),
        json!(expected)
    );
    assert_ne!(session.suggest("{}").unwrap(), before);
}

#[test]
fn exported_eq_and_gain_start_from_adopted_pcm_without_accumulation() {
    let (mut session, raw) = fixture();
    session.adopt_master(raw.clone(), "{}", "{}").unwrap();
    let pointer = session.preview_ptr().unwrap();
    let baseline = session.export_wav("{}", "wav32f").unwrap();
    assert_eq!(bits(&decode(&baseline).samples), bits(&raw));
    let mut settings = Settings {
        eq_hold_loudness: false,
        eq_output_db: -3.,
        ..Settings::default()
    };
    for band in &mut settings.manual_eq {
        band.on = false;
    }
    settings.manual_eq[3].on = true;
    settings.manual_eq[3].freq = 1000.;
    settings.manual_eq[3].gain = 6.;
    settings.manual_eq[3].q = 1.4;
    let json = serde_json::to_string(&settings).unwrap();
    let edited = session.export_wav(&json, "wav32f").unwrap();
    let output = decode(&edited);
    assert_eq!(
        (output.sample_rate, output.channels, output.samples.len()),
        (48000, 2, raw.len())
    );
    assert!(output.samples.iter().all(|sample| sample.is_finite()));
    for channel in 0..2 {
        let change = |frequency| {
            20. * (amplitude(&output, frequency, channel)
                / amplitude(&decode(&baseline), frequency, channel))
            .log10()
        };
        assert!((change(1000.) - 3.).abs() < 0.1);
        assert!((change(8000.) + 3.).abs() < 0.2);
    }
    assert_ne!(edited, baseline);
    assert_eq!(session.export_wav(&json, "wav32f").unwrap(), edited);
    assert_eq!(session.preview_ptr().unwrap(), pointer);
    assert_eq!(bits(&session.raw.as_ref().unwrap().samples), bits(&raw));
    assert_eq!(session.export_wav("{}", "wav32f").unwrap(), baseline);
}

#[test]
fn reference_changes_preserve_recovery_pcm_but_require_fresh_master_for_export() {
    let (mut session, raw) = fixture();
    assert_eq!(session.reference_analysis().unwrap(), "null");
    session
        .set_reference(tones(44100, 1, [0.01, 0.02, 0.03]), 44100, 1)
        .unwrap();
    let baseline_reference = session.reference_analysis().unwrap();
    session.adopt_master(raw.clone(), "{}", "{}").unwrap();
    let before = state(&session);
    assert_eq!(session.reference_analysis().unwrap(), baseline_reference);
    let exported = session.export_wav("{}", "wav32f").unwrap();
    let report = session.report().unwrap();
    session
        .set_reference(tones(44100, 1, [0.01, 0.02, 0.03]), 44100, 1)
        .unwrap();
    assert!(!session.raw_reference_stale);
    assert_eq!(session.export_wav("{}", "wav32f").unwrap(), exported);
    session
        .set_reference(tones(48000, 2, [0.04, 0.01, 0.005]), 48000, 2)
        .unwrap();
    let reference = session.reference_analysis().unwrap();
    assert_ne!(reference, baseline_reference);
    assert_eq!(
        serde_json::from_str::<Value>(&reference).unwrap(),
        json!(session.reference.as_ref().unwrap())
    );
    assert_eq!(session.report().unwrap(), report);
    assert_eq!(state(&session)["raw_ptr"], before["raw_ptr"]);
    assert_eq!(state(&session)["raw"], before["raw"]);
    assert_eq!(state(&session)["input"], before["input"]);
    assert!(session.raw_reference_stale);
    assert_eq!(
        session.export_source().unwrap_err().to_string(),
        "Reference changed. Master the track again before exporting."
    );
    let stale = state(&session);
    session
        .set_reference(tones(48000, 2, [0.04, 0.01, 0.005]), 48000, 2)
        .unwrap();
    assert_eq!(
        state(&session),
        stale,
        "Reloading a reference must not revive a stale master"
    );
    assert!(session.adopt_master_checked(vec![0.], "{}", "{}").is_err());
    assert_eq!(
        state(&session),
        stale,
        "A failed master must not restore export eligibility"
    );
    session.adopt_master(raw, "{}", "{}").unwrap();
    assert!(!session.raw_reference_stale);
    assert_eq!(session.export_wav("{}", "wav32f").unwrap(), exported);
}
