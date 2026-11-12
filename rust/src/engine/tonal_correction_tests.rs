use super::*;
use crate::{analysis, audio::Audio, engine::planning, profiles};
use serde_json::json;
use std::path::Path;

fn dark_mix() -> (Analysis, serde_json::Value) {
    let mut random = 0x12345678_u32;
    let samples = (0..48000)
        .map(|_| {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            (random as f64 / u32::MAX as f64 * 0.2 - 0.1) as f32
        })
        .collect();
    let mut input = Audio::new(samples, 48000, 1).unwrap();
    biquad::Biquad::design("high_shelf", 1000., -30., 0.707, 48000).apply(&mut input);
    let profile = profiles::resolve("neutral", Path::new(".")).unwrap();
    (analysis::analyze(&input, &profile).unwrap(), profile)
}

#[test]
fn overlapping_shelves_and_bells_obey_the_combined_response_budget() {
    let (measurement, _) = dark_mix();
    let filters = corrective_eq(&measurement, &Settings::default(), 1., None);
    assert!(filters.len() >= 3);
    for i in 0..977 {
        let frequency = 21. * (19900_f64 / 21.).powf(i as f64 / 976.);
        assert!(response(&filters, 48000, frequency).abs() <= 2.501);
    }
}

#[test]
fn lower_intensity_reduces_capped_high_frequency_correction_and_zero_disables_it() {
    let (mut measurement, _) = dark_mix();
    measurement.valid.fill(true);
    measurement.spectrum.cutoff_hz = None;
    measurement.deviation_db[7..].fill(-35.);
    let settings = Settings::default();
    let full = corrective_eq(&measurement, &settings, 1., None);
    let light = corrective_eq(&measurement, &settings, 0.35, None);
    let full_response = response(&full, 48000, 8000.).abs();
    let light_response = response(&light, 48000, 8000.).abs();
    assert!(full_response > 0.5);
    assert!(light_response < full_response * 0.40);
    assert!(corrective_eq(&measurement, &settings, 0., None).is_empty());
}

#[test]
fn dynamic_correction_does_not_double_charge_the_static_cut_budget() {
    let (measurement, profile) = dark_mix();
    let settings = Settings {
        modules: json!({"dynamic_eq":{"mud_max_db":8.}}),
        ..Default::default()
    };
    let filters = vec![EqBand {
        kind: "bell".into(),
        freq: 300.,
        gain: -2.5,
        q: 0.707,
        on: true,
    }];
    let mut muddy = measurement;
    for issue in &mut muddy.issues {
        if issue.key == "mud" {
            issue.severity = 80.;
        }
    }
    let bands = planning::dynamic_bands(&muddy, &settings, &profile, 1., &filters);
    let mud = bands.iter().find(|band| band.lo == 180.).unwrap();
    assert!(mud.max_cut <= 0.6, "{}", mud.max_cut);
}

#[test]
fn reference_matching_keeps_its_own_budget_and_ignores_missing_reference_bands() {
    let (measurement, _) = dark_mix();
    let mut reference = measurement.clone();
    reference.measured_db[7] += 20.;
    reference.valid[9] = false;
    let filters = corrective_eq(&measurement, &Settings::default(), 1., Some(&reference));
    assert!(filters.iter().all(|band| band.freq != 16000.));
    assert!(response(&filters, 48000, 4000.) > 2.5);
    for i in 0..300 {
        let frequency = 20. * 1000_f64.powf(i as f64 / 299.);
        assert!(response(&filters, 48000, frequency).abs() <= 6.001);
    }
}
