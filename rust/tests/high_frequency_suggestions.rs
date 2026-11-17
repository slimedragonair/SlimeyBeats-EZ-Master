use mastercraft::{
    analysis::{self, Analysis, high_frequency::HighFrequencySummary},
    audio::Audio,
    config::EqBand,
    dsp::{biquad, eq_suggestion},
    profiles,
};
use rustfft::{FftPlanner, num_complex::Complex64};
use serde_json::Value;
use std::{f64::consts::TAU, path::Path};

const SECONDS: f64 = 8.;

#[derive(Clone, Copy)]
enum Noise {
    White,
    Pink,
    HighOnly,
}

fn random(state: &mut u32) -> f64 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state as f64 / u32::MAX as f64 * 2. - 1.
}

fn noise(rate: u32, seconds: f64, kind: Noise, treble_db: f64) -> Audio {
    // A fixed physical-length noise tile gives repeatable broadband fixtures
    // without a dependency on a random-number package or large track FFTs.
    let size = if rate > 50_000 { 32768 } else { 16384 };
    let mut bins = vec![Complex64::default(); size];
    let mut state = 0x2f91_b3d7_u32;
    for i in 1..size / 2 {
        let frequency = i as f64 * rate as f64 / size as f64;
        let included = frequency >= 20.
            && frequency < (rate as f64 * 0.49).min(20_000.)
            && (!matches!(kind, Noise::HighOnly) || (10_000. ..16_000.).contains(&frequency));
        if !included {
            continue;
        }
        let tilt = if matches!(kind, Noise::Pink) {
            (1000. / frequency).sqrt()
        } else {
            1.
        };
        let gain = if (10_000. ..16_000.).contains(&frequency) {
            10_f64.powf(treble_db / 20.)
        } else {
            1.
        };
        bins[i] = Complex64::new(random(&mut state), random(&mut state)) * tilt * gain;
        bins[size - i] = bins[i].conj();
    }
    FftPlanner::<f64>::new()
        .plan_fft_inverse(size)
        .process(&mut bins);
    let rms = (bins.iter().map(|value| value.re.powi(2)).sum::<f64>() / size as f64).sqrt();
    let samples = (0..(rate as f64 * seconds).round() as usize)
        .map(|i| (bins[i % size].re * 0.06 / rms) as f32)
        .collect();
    Audio::new(samples, rate, 1).unwrap()
}

fn treble_regions(rate: u32, spans: &[(f64, f64)]) -> Audio {
    let mut base = noise(rate, SECONDS, Noise::Pink, 0.);
    let high = noise(rate, SECONDS, Noise::Pink, 12.);
    for (i, value) in base.samples.iter_mut().enumerate() {
        let time = i as f64 / rate as f64;
        let blend = spans.iter().fold(0_f64, |weight, &(start, end)| {
            if time < start || time >= end {
                weight
            } else {
                // Smooth physical-time edges avoid manufacturing wideband clicks.
                let edge = ((time - start).min(end - time) / 0.01).clamp(0., 1.);
                weight.max(0.5 - 0.5 * (std::f64::consts::PI * edge).cos())
            }
        });
        *value = (*value as f64 * (1. - blend) + high.samples[i] as f64 * blend) as f32;
    }
    base
}

fn tones(rate: u32, seconds: f64, sweep: bool) -> Audio {
    let samples = (0..(rate as f64 * seconds) as usize)
        .map(|i| {
            let time = i as f64 / rate as f64;
            if sweep {
                let phase = TAU * (10_200. * time + 0.5 * 5200. / seconds * time * time);
                (0.07 * phase.sin() + 0.025 * (TAU * 3000. * time).sin()) as f32
            } else {
                (0.07 * (TAU * 12_000. * time).sin() + 0.025 * (TAU * 3000. * time).sin()) as f32
            }
        })
        .collect();
    Audio::new(samples, rate, 1).unwrap()
}

fn profile() -> Value {
    profiles::resolve("neutral", Path::new(".")).unwrap()
}

fn measure(audio: &Audio) -> Analysis {
    analysis::analyze(audio, &profile()).unwrap()
}

fn summary(measurement: &Analysis) -> &HighFrequencySummary {
    let high = measurement.spectrum.high_frequency.as_ref().unwrap();
    assert!(
        high.valid(measurement.sample_rate, measurement.duration_s),
        "{high:?}"
    );
    high
}

fn assert_negative(audio: &Audio, label: &str) {
    let measured = measure(audio);
    if let Some(high) = &measured.spectrum.high_frequency {
        assert!(!high.excess_candidate, "{label}: {high:?}");
        assert_eq!(high.suggested_cut_db, 0., "{label}: {high:?}");
    }
    assert_eq!(measured.severity("high_frequency_excess"), 0., "{label}");
}

fn response(bands: &[EqBand], rate: u32, frequency: f64) -> f64 {
    bands
        .iter()
        .filter(|band| band.on)
        .map(|band| biquad::from_band(band, rate).response_db(frequency, rate))
        .sum()
}

#[test]
fn balanced_white_and_pink_noise_are_negative_controls() {
    for rate in [32_000, 44_100, 48_000, 96_000] {
        for (kind, label) in [(Noise::White, "white"), (Noise::Pink, "pink")] {
            let audio = noise(rate, 4., kind, 0.);
            let measured = measure(&audio);
            let high = summary(&measured);
            assert!(!high.excess_candidate, "{rate}/{label}: {high:?}");
            assert_eq!(high.candidate_windows, 0, "{rate}/{label}: {high:?}");
            if matches!(kind, Noise::White) {
                assert!(high.p90_high_to_presence_db.abs() < 1., "{high:?}");
            }
        }
    }
}

#[test]
fn missing_midrange_anchor_abstains_despite_broad_loud_highs() {
    let measured = measure(&noise(48_000, 4., Noise::HighOnly, 0.));
    let high = summary(&measured);
    assert!(high.mean_high_power_fraction > 0.98, "{high:?}");
    assert!(high.mean_high_flatness > 0.18, "{high:?}");
    assert_eq!(high.candidate_windows, 0, "{high:?}");
    assert!(!high.excess_candidate);
}

#[test]
fn clean_tones_and_a_sweep_with_a_real_midrange_anchor_are_not_hiss() {
    for sweep in [false, true] {
        let measured = measure(&tones(48_000, SECONDS, sweep));
        let high = summary(&measured);
        assert!(high.p90_high_to_presence_db > 3., "{high:?}");
        assert!(high.mean_high_power_fraction > 0.18, "{high:?}");
        assert!(high.mean_high_flatness < 0.05, "{high:?}");
        assert!(!high.excess_candidate, "{high:?}");
    }
}

#[test]
fn a_single_brief_broadband_crash_is_not_a_sustained_or_recurrent_problem() {
    let measured = measure(&treble_regions(48_000, &[(2., 2.2)]));
    let high = summary(&measured);
    assert!(
        high.candidate_windows > 0,
        "The burst must actually be observed: {high:?}"
    );
    assert!(high.observed_candidate_seconds < 0.5, "{high:?}");
    assert!(high.candidate_groups <= 1, "{high:?}");
    assert!(!high.excess_candidate, "{high:?}");
    assert_eq!(high.suggested_cut_db, 0.);
}

#[test]
fn overlapping_candidate_windows_do_not_inflate_observed_support() {
    let measured = measure(&treble_regions(48_000, &[(2., 2.3)]));
    let high = summary(&measured);
    let summed_window_seconds = high.candidate_windows as f64 * 4096. / 48_000.;
    assert!(summed_window_seconds > 0.5, "{high:?}");
    assert!(high.observed_candidate_seconds < 0.5, "{high:?}");
    assert!(
        high.observed_candidate_seconds < summed_window_seconds * 0.7,
        "{high:?}"
    );
    assert!(!high.excess_candidate, "{high:?}");
}

#[test]
fn three_measured_treble_regions_work_below_the_sustained_fraction() {
    let measured = measure(&treble_regions(
        48_000,
        &[(1., 1.55), (3., 3.55), (5., 5.55)],
    ));
    let high = summary(&measured);
    assert!(high.excess_candidate, "{high:?}");
    assert_eq!(high.candidate_groups, 3, "{high:?}");
    assert!((0.05..0.3).contains(&high.candidate_fraction), "{high:?}");
    assert!(high.observed_candidate_seconds >= 0.5, "{high:?}");
    // The three measured spans total 1.65 s. Window edges may add at most two
    // analysis windows per span; the unobserved gaps must never become support.
    assert!(high.observed_candidate_seconds < 2.2, "{high:?}");
    assert!(measured.severity("high_frequency_excess") > 0.);
}

#[test]
fn sustained_boosted_treble_proposes_one_bounded_air_cut_at_each_supported_rate() {
    for rate in [32_000, 44_100, 48_000, 96_000] {
        let measured = measure(&noise(rate, 4., Noise::Pink, 12.));
        let high = summary(&measured);
        assert!(high.excess_candidate, "{rate}: {high:?}");
        assert_eq!(high.band_low_hz, 10_000.);
        assert_eq!(high.band_high_hz, 16_000_f64.min(rate as f64 * 0.49));
        assert!((-2.5..=-0.75).contains(&high.suggested_cut_db), "{high:?}");
        assert!(high.observed_candidate_seconds <= 4.);
        let proposed = eq_suggestion::proposal(&measured, &profile()).unwrap();
        assert_eq!(proposed.bands.len(), 7);
        assert_eq!(proposed.bands[6].kind, "high_shelf");
        assert_eq!(proposed.bands[6].freq, 10_000.);
        assert!(proposed.bands[6].on && proposed.bands[6].gain < 0.);
        assert!(!proposed.notes.is_empty());
        let upper = 20_000_f64.min(rate as f64 * 0.49);
        let mut deepest = 0_f64;
        for i in 0..=1024 {
            let frequency = 10_000. * (upper / 10_000.).powf(i as f64 / 1024.);
            let gain = response(&proposed.bands, rate, frequency);
            assert!(gain >= -3.0001, "{rate}/{frequency} Hz: {gain} dB");
            deepest = deepest.min(gain);
        }
        assert!(
            deepest < -0.5,
            "{rate}: proposal lost its correction: {deepest}"
        );
        assert_eq!(
            serde_json::to_vec(&proposed.bands).unwrap(),
            serde_json::to_vec(&eq_suggestion::suggest(&measured, &profile()).unwrap()).unwrap()
        );
    }
}

#[test]
fn gain_antiphase_duplicate_and_right_only_stereo_preserve_frequency_evidence() {
    for rate in [32_000, 44_100, 48_000, 96_000] {
        let mono = noise(rate, 3., Noise::Pink, 12.);
        let reference = measure(&mono);
        let expected = summary(&reference);
        assert!(expected.excess_candidate);
        for (gain, layout) in [(0.125, 0), (2., 0), (1., 1), (1., 2), (1., 3)] {
            let samples = mono
                .samples
                .iter()
                .flat_map(|&sample| {
                    let value = sample * gain;
                    match layout {
                        0 => vec![value],
                        1 => vec![value, value],
                        2 => vec![value, -value],
                        _ => vec![0., value],
                    }
                })
                .collect();
            let audio = Audio::new(samples, rate, if layout == 0 { 1 } else { 2 }).unwrap();
            let measured = measure(&audio);
            let actual = summary(&measured);
            assert_eq!(
                actual.candidate_windows, expected.candidate_windows,
                "{rate}/{gain}/{layout}"
            );
            assert_eq!(actual.candidate_groups, expected.candidate_groups);
            assert_eq!(actual.excess_candidate, expected.excess_candidate);
            assert_eq!(actual.suggested_cut_db, expected.suggested_cut_db);
            for (left, right) in [
                (
                    actual.mean_high_power_fraction,
                    expected.mean_high_power_fraction,
                ),
                (
                    actual.p90_high_to_presence_db,
                    expected.p90_high_to_presence_db,
                ),
                (actual.mean_high_flatness, expected.mean_high_flatness),
                (
                    actual.observed_candidate_seconds,
                    expected.observed_candidate_seconds,
                ),
            ] {
                assert!(
                    (left - right).abs() < 1e-5,
                    "{rate}/{gain}/{layout}: {left} vs {right}"
                );
            }
        }
    }
}

#[test]
fn silence_short_and_low_rate_audio_do_not_create_high_frequency_evidence() {
    assert_negative(
        &Audio::new(vec![0.; 48_000 * 4], 48_000, 1).unwrap(),
        "silence",
    );
    let short = measure(&noise(48_000, 1.5, Noise::Pink, 12.));
    assert!(short.spectrum.high_frequency.is_none());
    for rate in [8000, 16_000, 22_050, 24_000] {
        let measured = measure(&noise(rate, 3., Noise::Pink, 0.));
        assert!(measured.spectrum.high_frequency.is_none(), "{rate}");
        assert_eq!(measured.severity("high_frequency_excess"), 0.);
    }
}

#[test]
fn analysis_and_proposals_are_serially_deterministic_and_preserve_pcm_bits() {
    let audio = noise(48_000, 4., Noise::Pink, 12.);
    let before: Vec<_> = audio
        .samples
        .iter()
        .map(|sample| sample.to_bits())
        .collect();
    let first = measure(&audio);
    let second = measure(&audio);
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    let first_proposal = eq_suggestion::proposal(&first, &profile()).unwrap();
    let second_proposal = eq_suggestion::proposal(&second, &profile()).unwrap();
    assert_eq!(
        serde_json::to_vec(&first_proposal).unwrap(),
        serde_json::to_vec(&second_proposal).unwrap()
    );
    assert_eq!(
        before,
        audio
            .samples
            .iter()
            .map(|sample| sample.to_bits())
            .collect::<Vec<_>>()
    );
    let mut legacy = serde_json::to_value(&first).unwrap();
    legacy["spectrum"]
        .as_object_mut()
        .unwrap()
        .remove("high_frequency");
    let restored: Analysis = serde_json::from_value(legacy).unwrap();
    assert!(restored.spectrum.high_frequency.is_none());
    assert!(eq_suggestion::proposal(&restored, &profile()).is_ok());
}

#[test]
fn invalid_16_khz_octave_cannot_change_a_32_khz_proposal() {
    let mut measured = measure(&noise(32_000, 4., Noise::Pink, 12.));
    assert!(!measured.valid[9]);
    assert!(summary(&measured).excess_candidate);
    let expected =
        serde_json::to_vec(&eq_suggestion::proposal(&measured, &profile()).unwrap()).unwrap();
    for invalid_value in [-1000., 1000.] {
        measured.measured_db[9] = invalid_value;
        assert_eq!(
            expected,
            serde_json::to_vec(&eq_suggestion::proposal(&measured, &profile()).unwrap()).unwrap()
        );
    }
}

#[test]
fn existing_air_correction_is_not_double_charged_and_the_cascade_stays_bounded() {
    let mut measured = measure(&noise(48_000, 4., Noise::Pink, 12.));
    measured.measured_db.fill(0.);
    measured.measured_db[6..].fill(30.);
    let mut flat_profile = profile();
    flat_profile["tonal_db"] = serde_json::json!([0., 0., 0., 0., 0., 0., 0., 0., 0., 0.]);
    let guarded = eq_suggestion::proposal(&measured, &flat_profile).unwrap();
    let mut no_extra_evidence = measured.clone();
    no_extra_evidence.spectrum.high_frequency = None;
    let tonal = eq_suggestion::proposal(&no_extra_evidence, &flat_profile).unwrap();
    assert_eq!(
        serde_json::to_vec(&guarded.bands).unwrap(),
        serde_json::to_vec(&tonal.bands).unwrap()
    );
    let extra_cut = summary(&measured).suggested_cut_db;
    let mut incorrectly_added = guarded.bands.clone();
    incorrectly_added[6].gain += extra_cut;
    let mut deepest = 0_f64;
    let mut incorrect_deepest = 0_f64;
    for i in 0..=2048 {
        let frequency = 10_000. * 2_f64.powf(i as f64 / 2048.);
        let actual = response(&guarded.bands, 48_000, frequency);
        assert!(actual >= -3.0001, "{frequency} Hz: {actual} dB");
        deepest = deepest.min(actual);
        incorrect_deepest = incorrect_deepest.min(response(&incorrectly_added, 48_000, frequency));
    }
    assert!(
        deepest < -1.5,
        "The bounded proposal must retain a real correction: {deepest}"
    );
    assert!(
        incorrect_deepest < -3.,
        "The regression control must expose additive overcorrection: {incorrect_deepest}"
    );
}
