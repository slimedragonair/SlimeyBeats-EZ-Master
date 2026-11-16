use super::*;
use serde_json::{Value, json};

pub(super) struct Fixture {
    pub directory: tempfile::TempDir,
    pub source: PathBuf,
    pub audio: mastercraft::audio::Audio,
}
impl Fixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.wav");
        let samples = (0..16000)
            .flat_map(|i| {
                let v = (std::f64::consts::TAU * 280. * i as f64 / 16000.).sin() as f32 * 0.15;
                [v, v * 0.7]
            })
            .collect();
        let audio = mastercraft::audio::Audio::new(samples, 16000, 2).unwrap();
        mastercraft::wav_encoding::write_wav(
            &audio,
            File::create(&source).unwrap(),
            "wav32f",
            false,
        )
        .unwrap();
        Self {
            directory,
            source,
            audio,
        }
    }
    pub fn request(&self, operation: &str) -> Value {
        json!({"api_version":2,"project_root":self.directory.path(),"input":self.source,
            "out_dir":self.directory.path().join("out"),"operation":operation,"settings":{}})
    }
    pub fn call(&self, mut request: Value) -> Result<Value> {
        let stage = tempfile::Builder::new()
            .prefix("command-")
            .tempdir_in(self.directory.path())?;
        request["out_dir"] = json!(stage.path());
        let req = stage.path().join("request.json");
        let response = stage.path().join("response.json");
        std::fs::write(&req, serde_json::to_vec(&request)?)?;
        execute(&req, &response)?;
        // Keep successful response assets alive for this fixture's tests.
        let _ = stage.keep();
        Ok(serde_json::from_slice(&std::fs::read(response)?)?)
    }
}

#[test]
fn strict_request_rejects_unknown_fields_missing_state_and_mixed_operations() {
    let f = Fixture::new();
    let mut value = f.request("analyze");
    value["voice"] = json!({});
    assert!(f.call(value).is_err());
    let mut value = f.request("import");
    value["unknown"] = json!(true);
    assert!(f.call(value).is_err());
    let mut value = f.request("repair");
    value["repair"] =
        json!({"action":"declip","start_seconds":0,"end_seconds":1,"strength":0.5,"surprise":1});
    assert!(f.call(value).is_err());
    let mut value = f.request("master");
    value["settings"] = json!({"reference":"outside.wav"});
    assert!(f.call(value).is_err());
    let mut value = f.request("master");
    value["settings"] = json!({"intensitty":0.5});
    assert!(f.call(value).is_err());
    let mut value = f.request("region_edit");
    value["region"] =
        json!({"start_seconds":0,"end_seconds":0.5,"operation":{"kind":"silence","db":3}});
    assert!(f.call(value).is_err());
}

#[test]
fn limits_and_wav_header_refuse_audio_before_large_allocation() {
    let f = Fixture::new();
    for limits in [
        json!({"max_pcm_bytes":32}),
        json!({"max_duration_seconds":0.5}),
        json!({"waveform_bins":1025}),
        json!({"decode_timeout_seconds":0}),
    ] {
        let mut req = f.request("import");
        req["limits"] = limits;
        assert!(f.call(req).is_err());
    }
    let limits = request::Limits {
        max_duration_seconds: f64::NAN,
        ..request::Limits::default()
    };
    assert!(limits.validate().is_err());
    let invalid = mastercraft::audio::Audio {
        samples: vec![f32::NAN],
        sample_rate: 16000,
        channels: 1,
    };
    assert!(
        output::float(
            &invalid,
            f.directory.path(),
            "bad",
            output::Role::Original,
            &request::Limits::default()
        )
        .is_err()
    );
}

#[test]
fn canonical_containment_and_response_collision_preserve_existing_files() {
    let f = Fixture::new();
    let paths = paths::ProjectPaths::new(f.directory.path()).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let victim = outside.path().join("victim.wav");
    std::fs::write(&victim, b"unchanged").unwrap();
    assert!(paths.input(&victim).is_err());
    assert!(
        paths
            .directory(&outside.path().join("must-not-exist"))
            .is_err()
    );
    assert!(!outside.path().join("must-not-exist").exists());
    assert!(
        paths
            .input(&f.directory.path().join("..").join("outside.wav"))
            .is_err()
    );
    assert!(paths.response(&victim, f.directory.path()).is_err());
    assert_eq!(std::fs::read(&victim).unwrap(), b"unchanged");
    assert!(paths::ProjectPaths::new(std::path::Path::new("relative")).is_err());
}

#[cfg(unix)]
#[test]
fn symlinked_output_ancestors_and_escaping_inputs_are_refused() {
    let f = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    let link = f.directory.path().join("linked");
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    let paths = paths::ProjectPaths::new(f.directory.path()).unwrap();
    assert!(paths.directory(&link.join("out")).is_err());
    let input = outside.path().join("source.wav");
    std::fs::copy(&f.source, &input).unwrap();
    assert!(paths.input(&link.join("source.wav")).is_err());
    assert!(!outside.path().join("out").exists());
}

#[test]
fn waveform_includes_right_only_energy_and_final_partial_bin() {
    let audio = mastercraft::audio::Audio::new(vec![0., -0.8, 0., 0.9, 0., 0.4], 8000, 2).unwrap();
    let envelope = output::waveform(&audio, 2);
    assert_eq!(envelope.peaks, vec![[-0.8, 0.], [0., 0.9]]);
    assert_eq!(envelope.channel_peaks, vec![0., 0.9]);
    assert_eq!(output::waveform(&audio, 1024).peaks.len(), 3);
}

#[test]
fn serialized_reference_dimensions_are_validated_before_engine_indexing() {
    let f = Fixture::new();
    let profile = mastercraft::profiles::builtin()["neutral"].clone();
    let mut reference = mastercraft::analysis::analyze(&f.audio, &profile).unwrap();
    reference.measured_db.pop();
    let mut req = f.request("master");
    req["reference_analysis"] = json!(reference);
    assert!(f.call(req).is_err());
}

#[test]
fn module_controls_reject_unknown_types_and_unbounded_dsp_state() {
    let f = Fixture::new();
    for modules in [
        json!({"limiter":{"lookahead_ms":1e100}}),
        json!({"limiter":{"enabled":1}}),
        json!({"static_eq":{"surprise":true}}),
        json!({"resonance":{"max_notches":0.5}}),
        json!({"cleanup":{"hpf_hz":true}}),
    ] {
        let mut req = f.request("master");
        req["settings"] = json!({"modules":modules});
        assert!(f.call(req).is_err());
    }
    let value = json!({"modules":{"limiter":{"lookahead_ms":2.,"enabled":true},"static_eq":{"max_boost_db":3.}}});
    assert!(settings::parse(&value, f.directory.path()).is_ok());
}
