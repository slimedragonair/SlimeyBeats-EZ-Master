use super::*;
use crate::{dsp::limiter, meters};

fn near_nyquist(frames: usize, channels: usize) -> Audio {
    let samples = (0..frames)
        .flat_map(|i| {
            let t = i as f64;
            let window = (PI * t / (frames - 1) as f64).sin().powi(2);
            let signal = window
                * (0.61 * (2. * PI * 0.474 * t + 0.3).sin()
                    + 0.39 * (2. * PI * 0.489 * t + 1.1).sin());
            vec![signal as f32; channels]
        })
        .collect();
    Audio::new(samples, 44100, channels).unwrap()
}

#[test]
fn reconstruction_agrees_with_an_analytic_high_frequency_signal() {
    let input = near_nyquist(8192, 1);
    let mut oracle = 0_f64;
    for index in 1024 * 32..7168 * 32 {
        let t = index as f64 / 32.;
        let window = (PI * t / 8191.).sin().powi(2);
        oracle = oracle.max(
            (window
                * (0.61 * (2. * PI * 0.474 * t + 0.3).sin()
                    + 0.39 * (2. * PI * 0.489 * t + 1.1).sin()))
            .abs(),
        );
    }
    let measured = peak(&input, 0.);
    assert!(
        (meters::db(measured / oracle)).abs() < 0.2,
        "{measured} / {oracle}"
    );
}

#[test]
fn recorded_peak_regression_matches_the_independent_reconstruction() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/cipher_peak_f32.json")).unwrap();
    let samples: Vec<_> = fixture["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sample| sample.as_f64().unwrap() as f32)
        .collect();
    let input = Audio::new(samples, fixture["sample_rate"].as_u64().unwrap() as u32, 1).unwrap();
    let expected = fixture["expected_8x_kaiser64_dbtp"].as_f64().unwrap();
    let mut old = ebur128::EbuR128::new(1, input.sample_rate, ebur128::Mode::TRUE_PEAK).unwrap();
    old.add_frames_f32(&input.samples).unwrap();
    assert!(meters::db(old.true_peak(0).unwrap()) < -1.);
    assert!((meters::db(peak(&input, 0.)) - expected).abs() < 0.02);
    let mut protected = input;
    limiter::guard(&mut protected, -1.).unwrap();
    assert!(meters::db(peak(&protected, 0.)) <= -1.);
}

#[test]
fn final_guard_protects_an_analytic_reconstruction_at_multiple_rates() {
    for rate in [8000, 44100, 48000, 96000, 192000] {
        let mut input = near_nyquist(8192, 2);
        input.sample_rate = rate;
        let original = peak(&input, 0.);
        let trim = limiter::guard(&mut input, -1.).unwrap();
        assert!(trim <= 0.);
        assert!(meters::db(peak(&input, 0.)) <= -1.);
        assert!(meters::db(original) > -0.5);
        assert_eq!(input.frames(), 8192);
    }
}

#[test]
fn hot_start_end_and_very_short_files_include_the_filter_tail() {
    for samples in [
        vec![1., -1., 1.],
        vec![0., 0., 1., -1.],
        vec![1., -1., 0., 0.],
    ] {
        let input = Audio::new(samples, 48000, 1).unwrap();
        let envelope = envelope(&input);
        assert!(
            (peak(&input, 0.) - envelope.iter().copied().fold(0_f32, f32::max) as f64).abs() < 1e-6
        );
        let mut protected = input.clone();
        limiter::guard(&mut protected, -1.).unwrap();
        assert!(meters::db(peak(&protected, 0.)) <= -1.);
    }
}

#[test]
fn silence_is_zero_and_stereo_matches_channel_maximum() {
    assert_eq!(peak(&Audio::new(vec![0.; 1024], 48000, 1).unwrap(), 0.), 0.);
    let mono = near_nyquist(1024, 1);
    let mut stereo = near_nyquist(1024, 2);
    for frame in stereo.samples.as_chunks_mut::<2>().0 {
        frame[0] *= 0.1;
    }
    assert!((peak(&mono, 0.) - peak(&stereo, 0.)).abs() < 1e-7);
}
