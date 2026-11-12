use mastercraft::{
    audio::{self, Audio},
    config::Settings,
    dsp::{
        biquad, convolution,
        dynamics::{self, Compression},
        limiter::{self, LimiterConfig},
        low_end, manual_eq, oversampling,
    },
    engine, export, meters, profiles,
};
use std::{f64::consts::PI, path::Path};

fn tone(sr: u32, seconds: f64, freq: f64, amplitude: f64, channels: usize) -> Audio {
    let n = (sr as f64 * seconds) as usize;
    Audio::new(
        (0..n)
            .flat_map(|i| {
                vec![(amplitude * (2. * PI * freq * i as f64 / sr as f64).sin()) as f32; channels]
            })
            .collect(),
        sr,
        channels,
    )
    .unwrap()
}
fn limiter_config() -> LimiterConfig {
    LimiterConfig {
        target: -14.,
        ceiling: -1.,
        max_gr: 6.,
        max_makeup: 18.,
        lookahead_ms: 2.,
        release_ms: 30.,
        slow_ms: 250.,
        enabled: true,
    }
}

#[test]
fn standard_one_khz_sine_reads_expected_loudness() {
    let x = tone(48000, 3., 1000., 0.1, 1);
    let m = meters::measure(&x).unwrap();
    assert!((m.integrated_lufs + 23.0).abs() < 0.15, "{m:?}");
    assert!((m.true_peak_dbtp + 20.).abs() < 0.1);
}
#[test]
fn stereo_loudness_has_correct_channel_energy() {
    let mono = tone(44100, 1., 1000., 0.1, 1);
    let stereo = tone(44100, 1., 1000., 0.1, 2);
    assert!(
        (meters::integrated(&stereo).unwrap() - meters::integrated(&mono).unwrap() - 3.0103).abs()
            < 0.01
    );
}
#[test]
fn silence_is_not_amplified_or_nan() {
    let x = Audio::new(vec![0.; 48000], 48000, 1).unwrap();
    let s = Settings::default();
    let p = profiles::resolve("neutral", Path::new(".")).unwrap();
    let out = engine::master(&x, &s, &p, None, "silence", &|_, _| {}, &|| false).unwrap();
    assert_eq!(out.audio.samples, x.samples);
    assert_eq!(out.report.output.metrics.integrated_lufs, -70.);
    assert!(serde_json::to_string(&out.report).is_ok());
}
#[test]
fn very_short_float_overs_still_obey_ceiling() {
    let x = Audio::new(vec![3., -2., 1., -0.5], 48000, 1).unwrap();
    let s = Settings::default();
    let p = profiles::resolve("neutral", Path::new(".")).unwrap();
    let out = engine::master(&x, &s, &p, None, "short", &|_, _| {}, &|| false).unwrap();
    assert!(meters::measure(&out.audio).unwrap().true_peak_dbtp <= -1.);
}
#[test]
fn invalid_frames_nonfinite_and_multichannel_fail() {
    assert!(Audio::new(vec![f32::NAN], 48000, 1).is_err());
    assert!(Audio::new(vec![f32::INFINITY], 48000, 1).is_err());
    assert!(Audio::new(vec![0.; 5], 48000, 2).is_err());
    assert!(Audio::new(vec![0.; 12], 48000, 6).is_err());
    assert!(Audio::new(vec![], 48000, 1).is_err());
}
#[test]
fn fft_convolution_preserves_impulse_alignment() {
    let mut x = Audio::new(vec![0.; 20000], 48000, 1).unwrap();
    x.samples[6000] = 1.;
    let h = convolution::lowpass_kernel(120., 48000);
    let y = convolution::centered(&x, &h);
    for (i, &tap) in h.iter().enumerate() {
        assert!((y.samples[6000 + i - h.len() / 2] - tap).abs() < 1e-5);
    }
    assert!(y.samples[0..2000].iter().all(|v| v.abs() < 1e-6));
}
#[test]
fn multiband_with_unity_reconstructs_exact_input() {
    let x = tone(48000, 0.5, 1133., 0.2, 2);
    let mut y = x.clone();
    let c = Compression {
        threshold_db: 0.,
        ratio: 1.,
        attack_ms: 10.,
        release_ms: 100.,
        max_gr_db: 0.,
        auto_release: false,
    };
    dynamics::multiband(&mut y, &[120., 2500.], &[c.clone(), c.clone(), c]);
    let error = x
        .samples
        .iter()
        .zip(&y.samples)
        .map(|(a, b)| (a - b).abs())
        .fold(0_f32, f32::max);
    assert!(error < 1e-6, "reconstruction error {error}");
}
#[test]
fn flat_manual_eq_is_exact_passthrough() {
    let x = tone(48000, 0.25, 233., 0.3, 2);
    let mut y = x.clone();
    manual_eq::apply(&mut y, &Settings::default()).unwrap();
    assert_eq!(x.samples, y.samples);
}
#[test]
fn eq_boost_is_peak_guarded_and_duration_preserved() {
    let mut y = tone(48000, 1., 1000., 0.8, 2);
    let n = y.frames();
    let mut s = Settings::default();
    s.manual_eq[3].freq = 1000.;
    s.manual_eq[3].gain = 18.;
    s.eq_hold_loudness = false;
    manual_eq::apply(&mut y, &s).unwrap();
    assert_eq!(y.frames(), n);
    assert!(meters::measure(&y).unwrap().true_peak_dbtp <= -1.);
}
#[test]
fn limiter_hits_reachable_loudness_at_three_sample_rates() {
    for sr in [22050, 44100, 96000] {
        let x = tone(sr, 1., 997., 0.05, 2);
        let (y, info) = limiter::limit(&x, &limiter_config(), &|| false).unwrap();
        assert!(
            (meters::integrated(&y).unwrap() + 14.).abs() < 0.25,
            "{sr}: {info:?}"
        );
        assert!(meters::measure(&y).unwrap().true_peak_dbtp <= -1.);
        assert_eq!(x.frames(), y.frames());
    }
}
#[test]
fn limiter_detects_intersample_peaks() {
    let x = tone(48000, 0.6, 12000., 0.99, 2);
    let envelope = oversampling::peak_envelope(&x);
    assert!(envelope.iter().copied().fold(0_f32, f32::max) > 0.98);
    let (y, _) = limiter::limit(&x, &limiter_config(), &|| false).unwrap();
    assert!(meters::measure(&y).unwrap().true_peak_dbtp <= -1.);
}
#[test]
fn limiter_gain_never_exceeds_required_gain() {
    let env: Vec<_> = (0..5000)
        .map(|i| {
            if [0, 57, 2222, 4999].contains(&i) {
                2.0
            } else {
                0.05
            }
        })
        .collect();
    let cfg = limiter_config();
    let gain = limiter::gain_curve(&env, 48000, 2., &cfg);
    for (&e, &g) in env.iter().zip(&gain) {
        assert!(g as f64 <= (meters::lin(-1.2) / (e as f64 * 2.)).min(1.) + 1e-7);
    }
}
#[test]
fn low_gain_cap_does_not_chase_noise() {
    let x = tone(48000, 1., 1000., 0.00001, 1);
    let (y, info) = limiter::limit(&x, &limiter_config(), &|| false).unwrap();
    assert!(meters::integrated(&y).unwrap() - meters::integrated(&x).unwrap() <= 18.01);
    assert!(!info.reachable);
}
#[test]
fn cancellation_stops_before_processing() {
    let x = tone(48000, 0.5, 1000., 0.1, 2);
    let p = profiles::resolve("neutral", Path::new(".")).unwrap();
    assert!(
        engine::master(
            &x,
            &Settings::default(),
            &p,
            None,
            "cancel",
            &|_, _| {},
            &|| true
        )
        .is_err()
    );
}
#[test]
fn sub_duck_holds_for_milliseconds_without_double_conversion() {
    let x = tone(48000, 1., 40., 0.2, 1);
    let mut y = x.clone();
    low_end::duck(&mut y, &[12000], 2.);
    let energy = |a: &Audio| a.samples[13000..14400].iter().map(|v| v * v).sum::<f32>();
    assert!(
        energy(&y) / energy(&x) < 0.8,
        "60ms hold must still attenuate 20–50ms after the hit"
    );
}
#[test]
fn filter_response_matches_requested_gain() {
    let b = biquad::Biquad::design("bell", 1000., 6., 1., 48000);
    assert!((b.response_db(1000., 48000) - 6.).abs() < 1e-8);
}
#[test]
fn profile_blends_resolve_and_reject_bad_weights() {
    let p = profiles::resolve("metal:0.6+edm:0.4", Path::new(".")).unwrap();
    assert_eq!(profiles::target_curve(&p).unwrap().len(), 10);
    assert!(profiles::resolve("metal:-1+edm:2", Path::new(".")).is_err());
    assert!(profiles::resolve("bogus", Path::new(".")).is_err());
}
#[test]
fn settings_reject_unsafe_paths_and_values() {
    let mut s = Settings {
        suffix: "../oops".into(),
        ..Settings::default()
    };
    assert!(s.validate().is_err());
    s.suffix = "_master".into();
    s.intensity = f64::NAN;
    assert!(s.validate().is_err());
}
#[test]
fn outputs_never_overwrite_even_same_basename() {
    let dir = tempfile::tempdir().unwrap();
    let s = Settings {
        out_dir: dir.path().into(),
        formats: vec!["wav16".into(), "wav24".into(), "wav32f".into()],
        ..Settings::default()
    };
    let x = tone(48000, 0.5, 1000., 0.1, 2);
    let first = export::export(&x, Path::new("same.wav"), &s, &|| false).unwrap();
    let original = std::fs::read(first[0]["path"].as_str().unwrap()).unwrap();
    let next = export::export(&x, Path::new("same.wav"), &s, &|| false).unwrap();
    assert_ne!(first[0]["path"], next[0]["path"]);
    assert_eq!(
        original,
        std::fs::read(first[0]["path"].as_str().unwrap()).unwrap()
    );
    for item in first.iter().chain(&next) {
        let y = audio::load(Path::new(item["path"].as_str().unwrap())).unwrap();
        assert_eq!(y.frames(), x.frames());
        assert!(meters::measure(&y).unwrap().true_peak_dbtp <= -1.);
    }
}
#[test]
fn truncated_wav_reports_error() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("broken.wav");
    std::fs::write(&p, b"RIFF\xff\xff\xff\xffWAVEfmt ").unwrap();
    assert!(audio::load(&p).is_err());
}

#[test]
fn legacy_settings_accept_null_profile_targets_and_single_format() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.json");
    std::fs::write(&path,r#"{"general":{"profile":"rock","target_lufs":null,"ceiling_dbtp":null,"format":"wav16","formats":[],"reference":"","out_dir":""},"modules":{"headroom":{"enabled":false}},"safety":{"enabled":false}}"#).unwrap();
    let s = Settings::load(&path).unwrap();
    assert_eq!(s.profile, "rock");
    assert_eq!(s.ceiling_dbtp, -1.);
    assert!(s.target_lufs.is_none());
    assert_eq!(s.formats, vec!["wav16"]);
    assert!(!s.enabled("headroom"));
    assert!(!s.safety);
    assert!(s.reference.is_none());
}
#[test]
fn unsupported_eq_band_counts_fail_before_gui_or_engine_use() {
    let mut s = Settings::default();
    s.manual_eq.clear();
    assert!(s.validate().is_err());
}
#[test]
fn full_pipeline_preserves_length_and_ceiling_for_realistic_transients() {
    let mut x = tone(48000, 3., 220., 0.05, 2);
    for start in [12000, 36000, 60000, 84000, 108000] {
        for i in 0..12000 {
            let t = i as f64 / 48000.;
            let v = (0.7 * (2. * PI * (55. + 100. * (-t * 40.).exp()) * t).sin() * (-t * 20.).exp())
                as f32;
            for ch in 0..2 {
                x.samples[(start + i) * 2 + ch] += v;
            }
        }
    }
    let p = profiles::resolve("metal:0.5+edm:0.5", Path::new(".")).unwrap();
    let out = engine::master(
        &x,
        &Settings::default(),
        &p,
        None,
        "drums",
        &|_, _| {},
        &|| false,
    )
    .unwrap();
    assert_eq!(x.frames(), out.audio.frames());
    assert!(out.audio.samples.iter().all(|s| s.is_finite()));
    assert!(meters::measure(&out.audio).unwrap().true_peak_dbtp <= -1.);
    assert!(!out.report.log.is_empty());
}

#[test]
fn eq_preset_import_is_validated_before_replacing_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eq.json");
    let mut state = Settings::default();
    let mut bands = state.manual_eq.clone();
    bands[3].gain = 2.;
    std::fs::write(
        &path,
        serde_json::to_vec(
            &serde_json::json!({"bands":bands,"hold_loudness":false,"out_gain_db":-1.}),
        )
        .unwrap(),
    )
    .unwrap();
    state.import_eq_preset(&path).unwrap();
    assert_eq!(state.manual_eq[3].gain, 2.);
    assert!(!state.eq_hold_loudness);
    assert_eq!(state.eq_output_db, -1.);
    let previous = serde_json::to_vec(&state).unwrap();
    std::fs::write(&path, r#"{"bands":[]}"#).unwrap();
    assert!(state.import_eq_preset(&path).is_err());
    assert_eq!(previous, serde_json::to_vec(&state).unwrap());
}
