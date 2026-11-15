use super::{RegionEdit, RegionOperation, render};
use crate::audio::Audio;
use serde_json::json;
use std::cell::Cell;

const RATE: u32 = 8000;
const DOUBLE_DB: f64 = 6.020599913279624;

fn audio(samples: &[f32], channels: usize) -> Audio {
    Audio::new(samples.to_vec(), RATE, channels).unwrap()
}

fn edit(start_frame: f64, end_frame: f64, operation: RegionOperation) -> RegionEdit {
    RegionEdit {
        start_seconds: start_frame / RATE as f64,
        end_seconds: end_frame / RATE as f64,
        operation,
    }
}

fn bits(samples: &[f32]) -> Vec<u32> {
    samples.iter().map(|sample| sample.to_bits()).collect()
}

fn assert_revision(input: &Audio, edit: &RegionEdit, expected: &[f32]) -> Audio {
    let original = bits(&input.samples);
    let output = render(input, edit, &|| false).unwrap();
    assert_eq!(bits(&output.samples), bits(expected));
    assert_eq!(output.sample_rate, input.sample_rate);
    assert_eq!(output.channels, input.channels);
    assert_eq!(output.frames(), input.frames());
    assert_eq!(bits(&input.samples), original, "source was mutated");
    output
}

#[test]
fn silence_changes_only_selected_stereo_frames() {
    let input = audio(&[-0., f32::MIN_POSITIVE, 1., -2., 3., -4., 5., -6.], 2);
    assert_revision(
        &input,
        &edit(1., 3., RegionOperation::Silence),
        &[-0., f32::MIN_POSITIVE, 0., 0., 0., 0., 5., -6.],
    );
}

#[test]
fn fade_in_shares_linear_envelope_and_includes_both_endpoints() {
    let input = audio(&[9., -9., 2., -4., 4., -8., 6., -12., 7., -7.], 2);
    assert_revision(
        &input,
        &edit(1., 4., RegionOperation::FadeIn),
        &[9., -9., 0., 0., 2., -4., 6., -12., 7., -7.],
    );
}

#[test]
fn fade_out_shares_linear_envelope_and_includes_both_endpoints() {
    let input = audio(&[9., -9., 2., -4., 4., -8., 6., -12., 7., -7.], 2);
    assert_revision(
        &input,
        &edit(1., 4., RegionOperation::FadeOut),
        &[9., -9., 2., -4., 2., -4., 0., 0., 7., -7.],
    );
}

#[test]
fn gain_uses_amplitude_db_without_normalization_or_limiting() {
    let input = audio(&[-0., 0.125, 0.75, -0.5, -2., 3.], 2);
    assert_revision(
        &input,
        &edit(1., 2., RegionOperation::Gain { db: DOUBLE_DB }),
        &[-0., 0.125, 1.5, -1., -2., 3.],
    );
    assert_revision(
        &input,
        &edit(1., 2., RegionOperation::Gain { db: -DOUBLE_DB }),
        &[-0., 0.125, 0.375, -0.25, -2., 3.],
    );
}

#[test]
fn gain_accepts_both_inclusive_limits() {
    let input = audio(&[1., -1.], 2);
    for (db, expected) in [(-60., 0.001_f32), (12., 3.9810717_f32)] {
        assert_revision(
            &input,
            &edit(0., 1., RegionOperation::Gain { db }),
            &[expected, -expected],
        );
    }
}

#[test]
fn reverse_preserves_stereo_channel_positions() {
    let input = audio(&[9., -9., 1., 10., 2., 20., 3., 30., 8., -8.], 2);
    assert_revision(
        &input,
        &edit(1., 4., RegionOperation::Reverse),
        &[9., -9., 3., 30., 2., 20., 1., 10., 8., -8.],
    );
}

#[test]
fn reverse_handles_even_and_odd_mono_regions() {
    let input = audio(&[0., 1., 2., 3., 4., 5.], 1);
    for (end, expected) in [
        (4., [0., 3., 2., 1., 4., 5.]),
        (5., [0., 4., 3., 2., 1., 5.]),
    ] {
        assert_revision(&input, &edit(1., end, RegionOperation::Reverse), &expected);
    }
}

#[test]
fn every_operation_handles_right_only_stereo() {
    let input = audio(&[0., 8., 0., 2., 0., 4., 0., 6., 0., 10.], 2);
    for (operation, expected) in [
        (RegionOperation::Silence, [0., 0., 0.]),
        (RegionOperation::FadeIn, [0., 2., 6.]),
        (RegionOperation::FadeOut, [2., 2., 0.]),
        (RegionOperation::Gain { db: DOUBLE_DB }, [4., 8., 12.]),
        (RegionOperation::Reverse, [6., 4., 2.]),
    ] {
        assert_revision(
            &input,
            &edit(1., 4., operation),
            &[
                0.,
                8.,
                0.,
                expected[0],
                0.,
                expected[1],
                0.,
                expected[2],
                0.,
                10.,
            ],
        );
    }
}

#[test]
fn every_operation_preserves_unselected_sample_bits() {
    let subnormal = f32::from_bits(1);
    let input = audio(&[-0., subnormal, 1., -2., 3., -4., -subnormal, -0.], 2);
    let original = bits(&input.samples);
    for operation in [
        RegionOperation::Silence,
        RegionOperation::FadeIn,
        RegionOperation::FadeOut,
        RegionOperation::Gain { db: -3. },
        RegionOperation::Reverse,
    ] {
        let mut output = render(&input, &edit(1., 3., operation), &|| false).unwrap();
        assert_eq!(bits(&output.samples[..2]), original[..2]);
        assert_eq!(bits(&output.samples[6..]), original[6..]);
        output.samples[0] = 42.;
        assert_eq!(bits(&input.samples), original);
    }
}

#[test]
fn fractional_times_round_to_nearest_frames_with_half_frames_upward() {
    let input = audio(&[10., 11., 12., 13., 14., 15.], 1);
    for (start, end, expected) in [
        (1.49, 4.49, [10., 0., 0., 0., 14., 15.]),
        (1.5, 4.5, [10., 11., 0., 0., 0., 15.]),
        (1.51, 4.51, [10., 11., 0., 0., 0., 15.]),
    ] {
        assert_revision(
            &input,
            &edit(start, end, RegionOperation::Silence),
            &expected,
        );
    }
}

#[test]
fn full_duration_includes_the_last_frame_at_supported_sample_rates() {
    for sample_rate in [8000, 44100, 48000, 384000] {
        let input = Audio::new(vec![1., 2., 3., 4., 5., 6.], sample_rate, 2).unwrap();
        let selection = RegionEdit {
            start_seconds: 0.,
            end_seconds: input.duration(),
            operation: RegionOperation::Reverse,
        };
        assert_revision(&input, &selection, &[5., 6., 3., 4., 1., 2.]);
    }
}

#[test]
fn single_frame_operations_have_explicit_results() {
    let input = audio(&[9., -9., 2., -4., 8., -8.], 2);
    for (operation, expected) in [
        (RegionOperation::Silence, [0., 0.]),
        (RegionOperation::FadeIn, [0., 0.]),
        (RegionOperation::FadeOut, [0., 0.]),
        (RegionOperation::Gain { db: DOUBLE_DB }, [4., -8.]),
        (RegionOperation::Reverse, [2., -4.]),
    ] {
        assert_revision(
            &input,
            &edit(1., 2., operation),
            &[9., -9., expected[0], expected[1], 8., -8.],
        );
    }
}

#[test]
fn two_frame_fades_reach_silence_and_original_level() {
    let input = audio(&[2., -4.], 1);
    assert_revision(&input, &edit(0., 2., RegionOperation::FadeIn), &[0., -4.]);
    assert_revision(&input, &edit(0., 2., RegionOperation::FadeOut), &[2., 0.]);
}

#[test]
fn invalid_times_are_rejected_before_rounding_can_hide_them() {
    let input = audio(&[1.; 8], 1);
    let duration = input.duration();
    for (start_seconds, end_seconds) in [
        (f64::NAN, duration),
        (0., f64::NAN),
        (f64::INFINITY, duration),
        (f64::NEG_INFINITY, duration),
        (0., f64::INFINITY),
        (0., f64::NEG_INFINITY),
        (-f64::MIN_POSITIVE, duration),
        (0., 0.),
        (duration, duration),
        (duration, 0.),
        (0., duration.next_up()),
    ] {
        let selection = RegionEdit {
            start_seconds,
            end_seconds,
            operation: RegionOperation::Silence,
        };
        assert!(selection.validate(&input).is_err(), "{selection:?}");
        assert!(render(&input, &selection, &|| false).is_err());
    }
}

#[test]
fn positive_time_spans_that_round_to_zero_frames_are_rejected() {
    let input = audio(&[1.; 8], 1);
    for (start, end) in [(0.1, 0.4), (1.6, 2.4), (7.6, 8.)] {
        let selection = edit(start, end, RegionOperation::Silence);
        let error = render(&input, &selection, &|| false).unwrap_err();
        assert!(error.to_string().contains("after rounding"));
        assert!(selection.validate(&input).is_err());
    }
}

#[test]
fn invalid_gain_is_rejected() {
    let input = audio(&[1., -1.], 2);
    for db in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -60.0001,
        12.0001,
    ] {
        let selection = edit(0., 1., RegionOperation::Gain { db });
        assert!(selection.validate(&input).is_err());
        assert!(render(&input, &selection, &|| false).is_err());
    }
}

#[test]
fn unsafe_gain_overflow_is_rejected_without_changing_the_source() {
    for value in [f32::MAX, -f32::MAX] {
        let input = audio(&[0.125, value, -0.], 1);
        let original = bits(&input.samples);
        let error = render(
            &input,
            &edit(1., 2., RegionOperation::Gain { db: 12. }),
            &|| false,
        )
        .unwrap_err();
        assert!(error.to_string().contains("overflowed"));
        assert_eq!(bits(&input.samples), original);
        assert_revision(
            &input,
            &edit(1., 2., RegionOperation::Gain { db: 0. }),
            &input.samples,
        );
    }
}

#[test]
fn invalid_audio_layouts_return_errors_without_panicking() {
    for (samples, sample_rate, channels) in [
        (vec![1.], RATE, 0),
        (vec![1.; 3], RATE, 3),
        (vec![1.; 3], RATE, 2),
        (vec![], RATE, 1),
        (vec![1.], 0, 1),
        (vec![1.], 7999, 1),
        (vec![1.], 384001, 1),
    ] {
        let input = Audio {
            samples,
            sample_rate,
            channels,
        };
        assert!(render(&input, &edit(0., 1., RegionOperation::Silence), &|| false).is_err());
    }
}

#[test]
fn nonfinite_pcm_is_rejected_even_outside_the_selection() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for invalid_index in [0, 1, 2] {
            let mut input = audio(&[1.; 3], 1);
            input.samples[invalid_index] = value;
            let original = bits(&input.samples);
            assert!(render(&input, &edit(1., 2., RegionOperation::Silence), &|| false).is_err());
            assert_eq!(bits(&input.samples), original);
        }
    }
}

#[test]
fn cancellation_is_checked_before_validation_and_copying() {
    let input = audio(&[1.; 3], 1);
    let error = render(
        &input,
        &edit(f64::NAN, 2., RegionOperation::Silence),
        &|| true,
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "Cancelled");
    assert_eq!(input.samples, [1.; 3]);
}

#[test]
fn cancellation_interrupts_copying_a_large_source() {
    let input = audio(&vec![0.5; 524_288], 2);
    let calls = Cell::new(0);
    let error = render(&input, &edit(0., 1., RegionOperation::Silence), &|| {
        calls.set(calls.get() + 1);
        calls.get() == 4
    })
    .unwrap_err();
    assert_eq!(error.to_string(), "Cancelled");
    assert_eq!(calls.get(), 4);
    assert!(input.samples.iter().all(|&sample| sample == 0.5));
}

#[test]
fn cancellation_interrupts_every_operation_after_copying() {
    let input = audio(&vec![0.5; 524_288], 2);
    let short_checks = Cell::new(0);
    render(&input, &edit(0., 1., RegionOperation::Silence), &|| {
        short_checks.set(short_checks.get() + 1);
        false
    })
    .unwrap();
    // The short edit has already copied the same source. A full-track edit
    // reaches this later check while still processing its selected frames.
    for operation in [
        RegionOperation::Silence,
        RegionOperation::FadeIn,
        RegionOperation::FadeOut,
        RegionOperation::Gain { db: 0. },
        RegionOperation::Reverse,
    ] {
        let calls = Cell::new(0);
        let selection = edit(0., input.frames() as f64, operation);
        let error = render(&input, &selection, &|| {
            calls.set(calls.get() + 1);
            calls.get() == short_checks.get()
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "Cancelled");
        assert_eq!(calls.get(), short_checks.get());
        assert!(input.samples.iter().all(|&sample| sample == 0.5));
    }
}

#[test]
fn edit_json_uses_tagged_snake_case_operations_and_rejects_unknown_fields() {
    for (operation, expected) in [
        (RegionOperation::Silence, json!({"kind": "silence"})),
        (RegionOperation::FadeIn, json!({"kind": "fade_in"})),
        (RegionOperation::FadeOut, json!({"kind": "fade_out"})),
        (
            RegionOperation::Gain { db: -3. },
            json!({"kind": "gain", "db": -3.}),
        ),
        (RegionOperation::Reverse, json!({"kind": "reverse"})),
    ] {
        let selection = edit(1., 2., operation);
        let encoded = serde_json::to_value(&selection).unwrap();
        assert_eq!(encoded["operation"], expected);
        let decoded: RegionEdit = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
        let mut extra_edit_field = encoded.clone();
        extra_edit_field["unexpected"] = json!(true);
        assert!(serde_json::from_value::<RegionEdit>(extra_edit_field).is_err());
        let mut extra_operation_field = encoded;
        extra_operation_field["operation"]["unexpected"] = json!(true);
        assert!(serde_json::from_value::<RegionEdit>(extra_operation_field).is_err());
    }
    for invalid in [
        json!({"kind": "Gain", "db": 0.}),
        json!({"kind": "gain"}),
        json!({"kind": "trim"}),
    ] {
        assert!(serde_json::from_value::<RegionOperation>(invalid).is_err());
    }
}

#[cfg(feature = "schemas")]
#[test]
fn edit_and_operations_expose_json_schemas() {
    let schema = serde_json::to_value(schemars::schema_for!(RegionEdit)).unwrap();
    assert!(schema["properties"]["start_seconds"].is_object());
    assert!(schema["properties"]["end_seconds"].is_object());
    let operation_schema = serde_json::to_value(schemars::schema_for!(RegionOperation)).unwrap();
    assert_eq!(operation_schema["oneOf"].as_array().unwrap().len(), 5);
}
