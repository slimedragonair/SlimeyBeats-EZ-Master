use mastercraft::{
    audio::Audio,
    studio::doctor::{self, RepairAction, RepairRequest},
};
use std::{cell::Cell, f64::consts::TAU};

fn tone(seconds: f64, frequency: f64, level: f64) -> Audio {
    Audio::new(
        (0..(seconds * 48000.) as usize)
            .map(|i| (level * (TAU * frequency * i as f64 / 48000.).sin()) as f32)
            .collect(),
        48000,
        1,
    )
    .unwrap()
}
fn request(action: RepairAction, start: f64, end: f64, strength: f64) -> RepairRequest {
    RepairRequest {
        action,
        start_seconds: start,
        end_seconds: end,
        strength,
    }
}
fn bits(audio: &Audio) -> Vec<u32> {
    audio
        .samples
        .iter()
        .map(|sample| sample.to_bits())
        .collect()
}
fn amplitude(audio: &Audio, start: usize, end: usize, hz: f64) -> f64 {
    let (mut re, mut im) = (0., 0.);
    for i in start..end {
        let phase = TAU * hz * i as f64 / audio.sample_rate as f64;
        re += audio.samples[i * audio.channels] as f64 * phase.cos();
        im += audio.samples[i * audio.channels] as f64 * phase.sin();
    }
    2. * re.hypot(im) / (end - start) as f64
}

#[test]
fn explicit_low_mid_eq_works_without_a_finding_on_a_short_fixture() {
    let input = tone(0.2, 320., 0.2);
    assert!(
        !doctor::scan(&input, &|| false)
            .unwrap()
            .issues
            .iter()
            .any(|issue| issue.kind == doctor::IssueKind::LowMidBuildup)
    );
    let result = doctor::repair(
        &input,
        &request(RepairAction::ReduceLowMids, 0., 0.2, 0.5),
        &|| false,
    )
    .unwrap();
    assert!(result.changed_samples > 0);
    assert!(result.audio.rms() < input.rms());
    assert_eq!(result.audio.frames(), input.frames());
}

#[test]
fn low_mid_bell_is_proportional_bounded_and_does_not_accumulate() {
    let input = tone(2., 320., 0.2);
    for strength in [0.25, 0.5, 1.] {
        let edit = request(RepairAction::ReduceLowMids, 0.25, 1.75, strength);
        let a = doctor::repair(&input, &edit, &|| false).unwrap();
        let b = doctor::repair(&input, &edit, &|| false).unwrap();
        let gain = 20.
            * (amplitude(&a.audio, 24000, 48000, 320.) / amplitude(&input, 24000, 48000, 320.))
                .log10();
        assert!(
            (gain + 3. * strength).abs() < 0.02,
            "gain={gain}, strength={strength}"
        );
        assert_eq!(bits(&a.audio), bits(&b.audio));
        assert_eq!(a.peak_trim_db, 0.);
    }
}

#[test]
fn low_mid_eq_preserves_unselected_pcm_and_edge_samples_exactly() {
    let mut mono = tone(2., 320., 0.2);
    mono.samples[0] = -0.;
    let input = Audio::new(
        mono.samples
            .iter()
            .flat_map(|&sample| [sample, -sample])
            .collect(),
        48000,
        2,
    )
    .unwrap();
    let original = bits(&input);
    let result = doctor::repair(
        &input,
        &request(RepairAction::ReduceLowMids, 0.5, 1.5, 1.),
        &|| false,
    )
    .unwrap();
    let after = bits(&result.audio);
    assert_eq!(bits(&input), original);
    assert_eq!(&after[..48000], &original[..48000]);
    assert_eq!(&after[144000..], &original[144000..]);
    assert_eq!(&after[48000..48002], &original[48000..48002]);
    assert_eq!(&after[143998..144000], &original[143998..144000]);
    assert_eq!(
        (result.audio.channels, result.audio.sample_rate),
        (2, 48000)
    );
    let actual = after
        .iter()
        .zip(original)
        .filter(|(a, b)| **a != *b)
        .count();
    assert_eq!(result.changed_samples, actual);
    for frame in 24000..72000 {
        assert!(
            (result.audio.samples[2 * frame] + result.audio.samples[2 * frame + 1]).abs() < 1e-8
        );
    }
}

#[test]
fn low_mid_edges_are_click_safe_and_filter_state_is_warmed() {
    let input = tone(1., 320., 0.2);
    let result = doctor::repair(
        &input,
        &request(RepairAction::ReduceLowMids, 0.2, 0.8, 1.),
        &|| false,
    )
    .unwrap();
    for frame in [9600, 38399] {
        assert_eq!(
            result.audio.samples[frame].to_bits(),
            input.samples[frame].to_bits()
        );
        let neighbor = if frame == 9600 { frame + 1 } else { frame - 1 };
        assert!((result.audio.samples[neighbor] - input.samples[neighbor]).abs() < 1e-6);
    }
}

#[test]
fn both_operations_are_bit_exact_at_zero_strength_including_negative_zero() {
    let mut input = tone(1., 1000., 1.2);
    input
        .samples
        .iter_mut()
        .for_each(|sample| *sample = sample.clamp(-1., 1.));
    input.samples[7] = -0.;
    for action in [RepairAction::Declip, RepairAction::ReduceLowMids] {
        let result = doctor::repair(&input, &request(action, 0.1, 0.9, 0.), &|| false).unwrap();
        assert_eq!(bits(&input), bits(&result.audio));
        assert_eq!(result.changed_samples, 0);
        assert_eq!(result.peak_trim_db, 0.);
    }
}

#[test]
fn declip_changes_real_short_clipped_runs_without_changing_other_samples() {
    let mut input = tone(0.2, 1000., 1.2);
    input
        .samples
        .iter_mut()
        .for_each(|sample| *sample = sample.clamp(-1., 1.));
    let original = bits(&input);
    let result = doctor::repair(
        &input,
        &request(RepairAction::Declip, 0.02, 0.18, 1.),
        &|| false,
    )
    .unwrap();
    assert!(result.changed_samples > 0);
    assert!(result.peak_trim_db > 0.);
    assert!(result.audio.samples.iter().all(|sample| sample.abs() <= 1.));
    for (i, &sample) in input.samples.iter().enumerate() {
        if !(960..8640).contains(&i) || sample.abs() < 0.9999 {
            assert_eq!(
                sample.to_bits(),
                result.audio.samples[i].to_bits(),
                "frame {i}"
            );
        }
    }
    assert_eq!(bits(&input), original);
    let half = doctor::repair(
        &input,
        &request(RepairAction::Declip, 0.02, 0.18, 0.5),
        &|| false,
    )
    .unwrap();
    for i in 960..8640 {
        let expected = (input.samples[i] + result.audio.samples[i]) * 0.5;
        assert!((half.audio.samples[i] - expected).abs() < 1e-7);
    }
}

#[test]
fn declip_is_channel_independent_and_handles_both_polarities() {
    let mut mono = tone(0.2, 1000., 1.2);
    mono.samples
        .iter_mut()
        .for_each(|sample| *sample = sample.clamp(-1., 1.));
    let input = Audio::new(
        mono.samples
            .iter()
            .flat_map(|&sample| [0., sample])
            .collect(),
        48000,
        2,
    )
    .unwrap();
    let result = doctor::repair(&input, &request(RepairAction::Declip, 0., 0.2, 1.), &|| {
        false
    })
    .unwrap();
    assert!(result.changed_samples > 0);
    let mut positive = false;
    let mut negative = false;
    for frame in 0..input.frames() {
        assert_eq!(result.audio.samples[2 * frame].to_bits(), 0_f32.to_bits());
        let before = input.samples[2 * frame + 1];
        if result.audio.samples[2 * frame + 1] != before {
            positive |= before > 0.;
            negative |= before < 0.;
        }
    }
    assert!(positive && negative);
}

#[test]
fn declip_abstains_on_clean_dc_long_boundary_and_overshoot_candidates() {
    let mut boundary = vec![1.; 3];
    boundary.extend_from_slice(&[0.8, 0.5, 0.2]);
    let mut ending = vec![0.2, 0.5, 0.8];
    ending.extend_from_slice(&[1.; 3]);
    for input in [
        tone(1., 50., 0.25),
        Audio::new(vec![1.; 4800], 48000, 1).unwrap(),
        Audio::new(vec![0.5, 0.8, 1.2, 1.2, 1.2, 0.8, 0.5], 48000, 1).unwrap(),
        Audio::new(boundary, 48000, 1).unwrap(),
        Audio::new(ending, 48000, 1).unwrap(),
    ] {
        let result = doctor::repair(
            &input,
            &request(RepairAction::Declip, 0., input.duration(), 1.),
            &|| false,
        )
        .unwrap();
        assert_eq!(bits(&input), bits(&result.audio));
        assert_eq!(result.changed_samples, 0);
        assert!(
            result
                .notes
                .iter()
                .any(|note| note.contains("No samples changed"))
        );
    }
}

#[test]
fn declip_does_not_repair_a_partial_run_selected_at_either_edge() {
    let input = Audio::new(vec![0.2, 0.5, 0.8, 1., 1., 1., 0.8, 0.5, 0.2], 48000, 1).unwrap();
    for (start, end) in [(4, 9), (0, 5), (4, 5)] {
        let result = doctor::repair(
            &input,
            &request(
                RepairAction::Declip,
                start as f64 / 48000.,
                end as f64 / 48000.,
                1.,
            ),
            &|| false,
        )
        .unwrap();
        assert_eq!(bits(&input), bits(&result.audio));
    }
}

#[test]
fn local_peak_protection_keeps_finite_extrema_finite_and_never_touches_outside() {
    let mut input = tone(0.2, 2000., 0.98);
    for frame in [1000, 1001, 1002, 1003] {
        input.samples[frame] = if frame % 2 == 0 { f32::MAX } else { f32::MIN };
    }
    let result = doctor::repair(
        &input,
        &request(RepairAction::ReduceLowMids, 0.01, 0.19, 1.),
        &|| false,
    )
    .unwrap();
    assert!(result.audio.samples.iter().all(|sample| sample.is_finite()));
    for (&before, &after) in input.samples.iter().zip(&result.audio.samples) {
        assert!(after.abs() <= before.abs().max(1.));
    }
    assert_eq!(&bits(&input)[..480], &bits(&result.audio)[..480]);
    assert_eq!(&bits(&input)[9120..], &bits(&result.audio)[9120..]);
    assert!(result.peak_trim_db.is_finite() && result.peak_trim_db >= 0.);
}

#[test]
fn invalid_parameters_layout_and_nonfinite_pcm_are_rejected() {
    let input = tone(1., 1000., 0.1);
    for (start, end, strength) in [
        (-1., 0.2, 1.),
        (0.5, 0.5, 1.),
        (0.8, 0.2, 1.),
        (0., 1.000001, 1.),
        (0., 1., -0.1),
        (0., 1., 1.1),
        (f64::NAN, 1., 1.),
        (0., f64::INFINITY, 1.),
        (0., 1., f64::NAN),
        (0., 1., f64::NEG_INFINITY),
        (0., 0.1 / 48000., 1.),
    ] {
        let edit = request(RepairAction::ReduceLowMids, start, end, strength);
        assert!(edit.validate(&input).is_err());
        assert!(doctor::repair(&input, &edit, &|| false).is_err());
    }
    for input in [
        Audio {
            samples: vec![0.; 2],
            sample_rate: 48000,
            channels: 0,
        },
        Audio {
            samples: vec![0.; 3],
            sample_rate: 48000,
            channels: 2,
        },
        Audio {
            samples: vec![0.; 2],
            sample_rate: 0,
            channels: 1,
        },
        Audio {
            samples: vec![f32::NAN; 480],
            sample_rate: 48000,
            channels: 1,
        },
        Audio {
            samples: vec![f32::INFINITY; 480],
            sample_rate: 48000,
            channels: 1,
        },
    ] {
        for strength in [0., 1.] {
            assert!(
                doctor::repair(
                    &input,
                    &request(RepairAction::ReduceLowMids, 0., 0.001, strength),
                    &|| false
                )
                .is_err()
            );
        }
    }
}

#[test]
fn cancellation_interrupts_copying_and_each_processing_path() {
    let input = tone(1., 1000., 0.2);
    for action in [RepairAction::Declip, RepairAction::ReduceLowMids] {
        let edit = request(action, 0., 1., 1.);
        assert_eq!(
            doctor::repair(&input, &edit, &|| true)
                .unwrap_err()
                .to_string(),
            "Cancelled"
        );
        for cancel_at in [4, 19] {
            let calls = Cell::new(0);
            let error = doctor::repair(&input, &edit, &|| {
                calls.set(calls.get() + 1);
                calls.get() == cancel_at
            })
            .unwrap_err();
            assert_eq!(error.to_string(), "Cancelled");
            assert_eq!(calls.get(), cancel_at);
        }
    }
}

#[test]
fn one_frame_eq_selection_is_a_safe_identity() {
    let input = tone(0.01, 320., 0.2);
    let result = doctor::repair(
        &input,
        &request(RepairAction::ReduceLowMids, 20. / 48000., 21. / 48000., 1.),
        &|| false,
    )
    .unwrap();
    assert_eq!(bits(&input), bits(&result.audio));
}

#[test]
fn serde_contract_is_strict_and_uses_snake_case_actions() {
    let edit = request(RepairAction::ReduceLowMids, 0., 0.2, 0.5);
    let value = serde_json::to_value(&edit).unwrap();
    assert_eq!(value["action"], "reduce_low_mids");
    let decoded: RepairRequest = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(decoded.strength, 0.5);
    let mut extra = value;
    extra["automatic_cut"] = serde_json::json!(true);
    assert!(serde_json::from_value::<RepairRequest>(extra).is_err());
    assert!(
        serde_json::from_str::<RepairRequest>(
            r#"{"action":"trim","start_seconds":0,"end_seconds":1,"strength":1}"#
        )
        .is_err()
    );
}

#[cfg(feature = "schemas")]
#[test]
fn request_and_action_expose_mcp_schemas() {
    let schema = serde_json::to_value(schemars::schema_for!(RepairRequest)).unwrap();
    assert!(schema["properties"]["strength"].is_object());
    let action = serde_json::to_value(schemars::schema_for!(RepairAction)).unwrap();
    assert_eq!(
        action["enum"],
        serde_json::json!(["declip", "reduce_low_mids"])
    );
}
