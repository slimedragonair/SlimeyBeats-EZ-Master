use super::{request_tests::Fixture, *};
use mastercraft::{
    audio,
    config::Settings,
    dsp::manual_eq,
    studio::{doctor, spectral_repair, voice_cleanup},
};
use serde_json::{Value, json};

fn decoded(response: &Value) -> audio::Audio {
    audio::load(std::path::Path::new(
        response["audio"]["path"].as_str().unwrap(),
    ))
    .unwrap()
}
fn exact(first: &audio::Audio, second: &audio::Audio) {
    assert_eq!(
        (first.frames(), first.sample_rate, first.channels),
        (second.frames(), second.sample_rate, second.channels)
    );
    assert!(
        first
            .samples
            .iter()
            .zip(&second.samples)
            .all(|(a, b)| a.to_bits() == b.to_bits())
    );
}

#[test]
fn import_is_exact_and_master_excludes_every_user_eq_filter_and_output_gain() {
    let f = Fixture::new();
    let original_bytes = std::fs::read(&f.source).unwrap();
    let imported = f.call(f.request("import")).unwrap();
    exact(&decoded(&imported), &f.audio);
    assert_eq!(imported["assets"][0]["role"], "original");
    assert_eq!(
        imported["waveform"]["peaks"].as_array().unwrap().len(),
        1024
    );
    let mut first = f.request("master");
    first["settings"] = json!({"safety":false,"intensity":0.});
    let mastered = f.call(first.clone()).unwrap();
    let mut settings = Settings::default();
    settings.manual_eq[0].kind = "low_cut".into();
    settings.manual_eq[0].freq = 500.;
    settings.eq_output_db = -10.;
    first["settings"]["manual_eq"] = json!(settings.manual_eq);
    first["settings"]["eq_output_db"] = json!(-10.);
    let alternate = f.call(first).unwrap();
    exact(&decoded(&mastered), &decoded(&alternate));
    assert_eq!(alternate["report"]["configuration"]["eq_output_db"], 0.);
    assert!(
        alternate["report"]["configuration"]["manual_eq"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["on"] == false)
    );
    assert_eq!(std::fs::read(&f.source).unwrap(), original_bytes);
}

#[test]
fn render_uses_shared_eq_without_remastering_or_accumulating_revisions() {
    let f = Fixture::new();
    let mut settings = Settings {
        eq_hold_loudness: false,
        eq_output_db: -3.,
        formats: vec!["wav16".into(), "wav24".into(), "wav32f".into()],
        ..Default::default()
    };
    settings.manual_eq[2].gain = -8.;
    let expected = manual_eq::render(&f.audio, &settings, &|| false).unwrap();
    let mut req = f.request("render");
    req["settings"] = json!(settings);
    let response = f.call(req.clone()).unwrap();
    exact(&decoded(&response), &expected.audio);
    exact(&decoded(&response), &decoded(&f.call(req).unwrap()));
    assert_eq!(response["assets"].as_array().unwrap().len(), 4);
    assert_eq!(response["assets"][0]["role"], "preview");
    assert!(response.get("report").is_none());
    for asset in response["assets"].as_array().unwrap() {
        assert_eq!(asset["frames"], 16000);
        assert_eq!(asset["sample_rate"], 16000);
        assert_eq!(asset["channels"], 2);
    }
}

#[test]
fn voice_repair_and_spectral_commands_match_shared_tools_and_reject_invalid_state() {
    let f = Fixture::new();
    let voice = voice_cleanup::VoiceSettings {
        highpass_hz: 0.,
        noise_reduction: 0.,
        deess: 0.,
        output_gain_db: -3.,
        hold_loudness: false,
        protect_peaks: false,
        ..Default::default()
    };
    let mut req = f.request("voice_cleanup");
    req["voice"] = json!(voice);
    exact(
        &decoded(&f.call(req.clone()).unwrap()),
        &voice_cleanup::render(&f.audio, &voice, &|| false)
            .unwrap()
            .audio,
    );
    req["voice"]["noise_region"] = json!({"start_seconds":0.8,"end_seconds":2.});
    assert!(f.call(req).is_err());
    let repair = doctor::RepairRequest {
        action: doctor::RepairAction::ReduceLowMids,
        start_seconds: 0.2,
        end_seconds: 0.6,
        strength: 0.6,
    };
    let mut req = f.request("repair");
    req["repair"] = json!(repair);
    let response = f.call(req.clone()).unwrap();
    exact(
        &decoded(&response),
        &doctor::repair(&f.audio, &repair, &|| false).unwrap().audio,
    );
    assert!(response.get("issues").is_some());
    req["repair"]["strength"] = json!(2.);
    assert!(f.call(req).is_err());
    let selection = spectral_repair::RepairSelection {
        start_seconds: 0.2,
        end_seconds: 0.6,
        low_hz: 100.,
        high_hz: 1000.,
        reduction_db: -6.,
        mode: spectral_repair::RepairMode::Attenuate,
    };
    let mut req = f.request("spectral_repair");
    req["spectral"] = json!(selection);
    exact(
        &decoded(&f.call(req.clone()).unwrap()),
        &spectral_repair::render(&f.audio, &selection, &|| false).unwrap(),
    );
    req["spectral"]["high_hz"] = json!(20000.);
    assert!(f.call(req).is_err());
}

#[test]
fn mix_controls_and_bank_dimensions_are_enforced_at_command_boundary() {
    let f = Fixture::new();
    let mut req = f.request("mix");
    req.as_object_mut().unwrap().remove("input");
    req["stems"] = json!([{"path":f.source,"kind":"vocals"},{"path":f.source,"kind":"drums"}]);
    req["mix"] = json!({"protect_peaks":false,"tracks":[{"kind":"vocals","gain_db":0,"balance":1,"mute":false,"solo":true},{"kind":"drums","gain_db":0,"balance":0,"mute":true,"solo":true}]});
    let response = f.call(req.clone()).unwrap();
    let audio = decoded(&response);
    assert!(
        audio
            .samples
            .as_chunks::<2>()
            .0
            .iter()
            .all(|frame| frame[0].abs() < 1e-7)
    );
    assert!(
        audio
            .samples
            .as_chunks::<2>()
            .0
            .iter()
            .zip(f.audio.samples.as_chunks::<2>().0.iter())
            .all(|(a, b)| a[1].to_bits() == b[1].to_bits())
    );
    req["mix"]["tracks"][1]["kind"] = json!("vocals");
    assert!(f.call(req.clone()).is_err());
    let short = f.directory.path().join("short.wav");
    let shorter = audio::Audio::new(vec![0.; 8000], 8000, 1).unwrap();
    mastercraft::wav_encoding::write_wav(&shorter, File::create(&short).unwrap(), "wav32f", false)
        .unwrap();
    req["stems"][1]["path"] = json!(short);
    assert!(f.call(req).is_err());
}

#[test]
fn spectrogram_and_region_edit_preserve_native_shapes_and_unselected_pcm() {
    let f = Fixture::new();
    let mut req = f.request("spectrogram");
    req["spectrogram"] = json!({"columns":32,"rows":16});
    let response = f.call(req).unwrap();
    assert_eq!(
        response["spectrogram"]["levels_dbfs"]
            .as_array()
            .unwrap()
            .len(),
        512
    );
    assert_eq!(
        response["spectrogram"]["frequencies_hz"]
            .as_array()
            .unwrap()
            .len(),
        16
    );
    let mut req = f.request("region_edit");
    req["region"] = json!({"start_seconds":0.2,"end_seconds":0.6,"operation":{"kind":"silence"}});
    let revised = decoded(&f.call(req).unwrap());
    assert_eq!(&revised.samples[..6400], &f.audio.samples[..6400]);
    assert!(revised.samples[6400..19200].iter().all(|v| *v == 0.));
    assert_eq!(&revised.samples[19200..], &f.audio.samples[19200..]);
}

#[test]
fn suggest_uses_shared_measurements_without_changing_audio_or_controls() {
    let f = Fixture::new();
    let before = std::fs::read(&f.source).unwrap();
    let response = f.call(f.request("suggest")).unwrap();
    let profile = mastercraft::profiles::builtin()["neutral"].clone();
    let measured = mastercraft::analysis::analyze(&f.audio, &profile).unwrap();
    let expected = mastercraft::dsp::eq_suggestion::proposal(&measured, &profile).unwrap();
    assert_eq!(response["suggestion"]["bands"], json!(expected.bands));
    assert_eq!(response["suggestion"]["notes"], json!(expected.notes));
    assert_eq!(
        response["suggestion"]["high_frequency"],
        json!(expected.high_frequency)
    );
    assert_eq!(response["assets"], json!([]));
    assert_eq!(std::fs::read(&f.source).unwrap(), before);
    assert!(response.get("waveform").is_none());
}

#[test]
fn opaque_stem_upload_paths_retain_original_names_and_explicit_kind_overrides() {
    let f = Fixture::new();
    let mut req = f.request("import_stems");
    req.as_object_mut().unwrap().remove("input");
    req["inputs"] = json!([
        {"path":f.source,"name":"Song (Vocals).mp3","kind":"part_12"},
        {"path":f.source,"name":"Song (Drums).wav"}
    ]);
    let response = f.call(req).unwrap();
    let assets = response["assets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|asset| asset["role"] == "stem")
        .collect::<Vec<_>>();
    assert_eq!(assets.len(), 2);
    assert!(
        assets
            .iter()
            .any(|asset| asset["kind"] == "part_12" && asset["name"] == "Song (Vocals)")
    );
    assert!(
        assets
            .iter()
            .any(|asset| asset["kind"] == "drums" && asset["name"] == "Song (Drums)")
    );
    for asset in assets {
        assert_eq!(asset["channel_peaks"].as_array().unwrap().len(), 2);
        let decoded = audio::load(std::path::Path::new(asset["path"].as_str().unwrap())).unwrap();
        exact(&decoded, &f.audio);
    }
}

#[test]
fn imported_bank_has_raw_sum_original_and_guarded_full_mix_primary_and_export() {
    use mastercraft::studio::stem_mix::{self, Stem, StemKind, StemSet};
    use std::sync::Arc;
    let f = Fixture::new();
    let mut loud = f.audio.clone();
    loud.gain(20.);
    let loud_path = f.directory.path().join("loud.wav");
    mastercraft::wav_encoding::write_wav(&loud, File::create(&loud_path).unwrap(), "wav32f", false)
        .unwrap();
    let bank = StemSet::new(vec![
        Stem {
            kind: StemKind::Vocals,
            audio: Arc::new(f.audio.clone()),
        },
        Stem {
            kind: StemKind::Drums,
            audio: Arc::new(loud),
        },
    ])
    .unwrap();
    let mut raw = bank.defaults();
    raw.protect_peaks = false;
    let expected_original = stem_mix::render(&bank, &raw, &|| false).unwrap();
    let expected_preview = stem_mix::render(&bank, &bank.defaults(), &|| false).unwrap();
    let mut req = f.request("import_stems");
    req.as_object_mut().unwrap().remove("input");
    req["stems"] = json!([{"path":f.source,"kind":"vocals"},{"path":loud_path,"kind":"drums"}]);
    let result = f.call(req).unwrap();
    let assets = result["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 4);
    let original = assets.iter().find(|a| a["role"] == "original").unwrap();
    let preview = assets.iter().find(|a| a["role"] == "preview").unwrap();
    assert_eq!(result["audio"]["path"], preview["path"]);
    let original_audio =
        audio::load(std::path::Path::new(original["path"].as_str().unwrap())).unwrap();
    exact(&original_audio, &expected_original);
    exact(&decoded(&result), &expected_preview);
    assert!(original["channel_peaks"][0].as_f64().unwrap() > 1.);
    assert!(
        mastercraft::meters::measure(&decoded(&result))
            .unwrap()
            .true_peak_dbtp
            <= -0.995
    );
    let mut render = f.request("render");
    render["input"] = original["path"].clone();
    render["settings"] = json!({"formats":["wav32f"]});
    let exported = f.call(render).unwrap();
    let expected_export =
        manual_eq::render(&expected_original, &Settings::default(), &|| false).unwrap();
    exact(&decoded(&exported), &expected_export.audio);
    assert!(decoded(&exported).rms() > f.audio.rms() * 2.);
}
