use super::*;
use clap::Parser;
use mastercraft::{analysis, audio::Audio, profiles};
use std::io::Write;

fn reference() -> Analysis {
    let audio = Audio::new(
        (0..4800).map(|i| (i as f32 * 0.0576).sin() * 0.2).collect(),
        48000,
        1,
    )
    .unwrap();
    analysis::analyze(&audio, &profiles::builtin()["neutral"]).unwrap()
}

#[test]
fn accepts_native_analysis_and_discards_reference_display_text() {
    let expected = reference();
    let mut file = tempfile::NamedTempFile::new().unwrap();
    serde_json::to_writer(&mut file, &expected).unwrap();
    let actual = load(file.path()).unwrap();
    assert_eq!(actual.measured_db, expected.measured_db);
    assert_eq!(actual.valid, expected.valid);
    assert_eq!(actual.metrics.correlation, expected.metrics.correlation);
    assert!(actual.issues.is_empty());
    assert_eq!(actual.suggested_profile, "neutral");
}

#[test]
fn rejects_invalid_geometry_metrics_and_vector_shapes() {
    let value = reference();
    let mut cases = Vec::new();
    macro_rules! invalid {
        ($name:ident, $body:expr) => {{
            let mut $name = value.clone();
            $body;
            cases.push($name);
        }};
    }
    invalid!(v, v.measured_db.pop());
    invalid!(v, v.deviation_db.push(0.));
    invalid!(v, v.valid.clear());
    invalid!(v, v.measured_db[0] = f64::NAN);
    invalid!(v, v.metrics.correlation = 1.1);
    invalid!(v, v.metrics.dc_offset.clear());
    invalid!(v, v.metrics.clipped_samples = usize::MAX);
    invalid!(v, v.duration_s = 0.);
    invalid!(v, v.sample_rate = 1);
    invalid!(v, v.channels = 3);
    invalid!(v, v.spectrum.freqs.swap(0, 1));
    invalid!(v, v.spectrum.db[0] = f64::INFINITY);
    invalid!(v, v.spectrum.db.clear());
    invalid!(v, v.spectrum.cutoff_hz = Some(48000.));
    invalid!(v, v.spectrum.octave_power[0] = -1.);
    invalid!(v, v.spectrum.sibilance_spike_fraction = 2.);
    for (index, case) in cases.iter().enumerate() {
        assert!(validate(case).is_err(), "case {index} was accepted");
    }
}

#[test]
fn rejects_oversized_malformed_missing_fields_and_nonregular_files() {
    for bytes in [
        vec![b' '; MAX_BYTES as usize + 1],
        b"{}".to_vec(),
        b"null".to_vec(),
        b"{\"measured_db\":[NaN]}".to_vec(),
    ] {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&bytes).unwrap();
        assert!(load(file.path()).is_err());
    }
    let directory = tempfile::tempdir().unwrap();
    assert!(load(directory.path()).is_err());
    assert!(load(&directory.path().join("missing.json")).is_err());
}

#[test]
fn reference_sources_are_mutually_exclusive() {
    use crate::cli_arguments::Arguments;
    assert!(
        Arguments::try_parse_from([
            "cli",
            "input.wav",
            "--reference",
            "ref.wav",
            "--reference-analysis",
            "ref.json"
        ])
        .is_err()
    );
    assert!(
        Arguments::try_parse_from(["cli", "input.wav", "--reference-analysis", "ref.json"]).is_ok()
    );
    assert!(Arguments::try_parse_from(["cli", "input.wav", "--reference", "ref.wav"]).is_ok());
}
