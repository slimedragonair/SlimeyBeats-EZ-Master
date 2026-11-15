use mastercraft::{
    audio::Audio,
    studio::doctor::{self, IssueKind},
};
use std::{cell::Cell, f64::consts::TAU};

fn tone(seconds: f64, frequency: f64, level: f64) -> Audio {
    Audio::new(
        (0..(seconds * 48000.) as usize)
            .map(|i| (level * (i as f64 * frequency * TAU / 48000.).sin()) as f32)
            .collect(),
        48000,
        1,
    )
    .unwrap()
}
fn has(audio: &Audio, kind: IssueKind) -> bool {
    doctor::scan(audio, &|| false)
        .unwrap()
        .issues
        .iter()
        .any(|issue| issue.kind == kind)
}
fn concatenate(parts: &[Audio]) -> Audio {
    Audio::new(
        parts
            .iter()
            .flat_map(|audio| audio.samples.iter().copied())
            .collect(),
        48000,
        1,
    )
    .unwrap()
}
fn silence(seconds: f64) -> Audio {
    tone(seconds, 1000., 0.)
}
fn broad_low_mids(seconds: f64) -> Audio {
    let samples = (0..(seconds * 48000.) as usize)
        .map(|frame| {
            let time = frame as f64 / 48000.;
            let low: f64 = (0..24)
                .map(|i| 0.018 * (TAU * (210. + 12. * i as f64) * time).sin())
                .sum();
            (low + 0.025 * (TAU * 1200. * time).sin() + 0.025 * (TAU * 2400. * time).sin()) as f32
        })
        .collect();
    Audio::new(samples, 48000, 1).unwrap()
}

#[test]
fn low_mid_findings_are_local_to_a_sustained_section_between_clear_audio() {
    let input = concatenate(&[
        tone(4., 1000., 0.2),
        broad_low_mids(3.),
        tone(4., 1000., 0.2),
    ]);
    let report = doctor::scan(&input, &|| false).unwrap();
    let issues: Vec<_> = report
        .issues
        .iter()
        .filter(|issue| issue.kind == IssueKind::LowMidBuildup)
        .collect();
    assert_eq!(issues.len(), 1);
    assert!(issues[0].start_seconds >= 3.8 && issues[0].start_seconds < 4.2);
    assert!(issues[0].end_seconds > 6.8 && issues[0].end_seconds <= 7.2);
    assert_eq!(issues[0].evidence["continuous_buildup_confirmed"], false);
    assert!(
        issues[0].evidence["unsampled_seconds_in_span"]
            .as_f64()
            .unwrap()
            >= 0.
    );
}

#[test]
fn low_mid_regions_do_not_merge_across_quiet_or_tonal_sections() {
    for middle in [silence(2.), tone(2., 320., 0.2)] {
        let input = concatenate(&[broad_low_mids(3.), middle, broad_low_mids(3.)]);
        let report = doctor::scan(&input, &|| false).unwrap();
        let issues: Vec<_> = report
            .issues
            .iter()
            .filter(|issue| issue.kind == IssueKind::LowMidBuildup)
            .collect();
        assert_eq!(issues.len(), 2);
        assert!(issues[0].end_seconds <= 3.2);
        assert!(issues[1].start_seconds >= 4.8);
    }
}

#[test]
fn clean_quarter_scale_50_hz_is_not_clipping() {
    let input = tone(3., 50., 0.25);
    let original = input.samples.clone();
    let report = doctor::scan(&input, &|| false).unwrap();
    assert!(
        !report
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::Clipping)
    );
    assert_eq!(input.samples, original);
}

#[test]
fn clean_full_scale_sine_is_not_a_flat_clipped_plateau() {
    assert!(!has(&tone(1., 50., 1.), IssueKind::Clipping));
}

#[test]
fn clipped_runs_are_absolute_pcm_measurements_and_have_eligible_repairs() {
    let mut input = tone(0.2, 1000., 1.2);
    input
        .samples
        .iter_mut()
        .for_each(|sample| *sample = sample.clamp(-1., 1.));
    let report = doctor::scan(&input, &|| false).unwrap();
    let clipped: Vec<_> = report
        .issues
        .iter()
        .filter(|issue| issue.kind == IssueKind::Clipping)
        .collect();
    assert!(!clipped.is_empty());
    assert!(
        clipped
            .iter()
            .all(|issue| issue.evidence["peak_absolute_sample"] == 1.)
    );
    assert!(clipped.iter().any(|issue| issue.repair.is_some()));
    assert!(
        clipped
            .iter()
            .all(|issue| issue.evidence["true_peak_measured"] == false)
    );
    assert!(
        clipped
            .iter()
            .all(|issue| issue.end_seconds > issue.start_seconds)
    );
}

#[test]
fn over_unity_single_sample_and_finite_extrema_are_reported_without_repair_claims() {
    for value in [1.0001_f32, -1.1, f32::MAX, f32::MIN] {
        let mut input = tone(0.02, 1000., 0.1);
        input.samples[20] = value;
        let report = doctor::scan(&input, &|| false).unwrap();
        let issue = report
            .issues
            .iter()
            .find(|issue| issue.kind == IssueKind::Clipping)
            .unwrap();
        assert_eq!(issue.evidence["above_unity_sample"], true);
        assert!(issue.repair.is_none());
        serde_json::to_string(&report).unwrap();
    }
}

#[test]
fn stereo_right_only_activity_is_not_silence_and_right_clipping_is_identified() {
    let mono = tone(2., 1000., 0.2);
    let mut input = Audio::new(
        mono.samples.iter().flat_map(|&v| [0., v]).collect(),
        48000,
        2,
    )
    .unwrap();
    assert!(!has(&input, IssueKind::Silence));
    for i in 100..104 {
        input.samples[2 * i + 1] = -1.;
    }
    let report = doctor::scan(&input, &|| false).unwrap();
    let clipped: Vec<_> = report
        .issues
        .iter()
        .filter(|issue| issue.kind == IssueKind::Clipping)
        .collect();
    assert_eq!(clipped.len(), 1);
    assert_eq!(clipped[0].evidence["channel"], 1);
}

#[test]
fn anti_phase_stereo_does_not_cancel_into_silence() {
    let mono = tone(1., 1000., 0.1);
    let input = Audio::new(
        mono.samples.iter().flat_map(|&v| [v, -v]).collect(),
        48000,
        2,
    )
    .unwrap();
    assert!(!has(&input, IssueKind::Silence));
}

#[test]
fn silence_threshold_duration_and_all_channel_requirement_are_absolute() {
    assert!(!has(&silence(0.74), IssueKind::Silence));
    for rate in [8000, 44100, 48000, 96000, 384000] {
        let input = Audio::new(vec![0.; rate as usize], rate, 1).unwrap();
        let report = doctor::scan(&input, &|| false).unwrap();
        let issues: Vec<_> = report
            .issues
            .iter()
            .filter(|issue| issue.kind == IssueKind::Silence)
            .collect();
        assert_eq!(issues.len(), 1);
        assert_eq!((issues[0].start_seconds, issues[0].end_seconds), (0., 1.));
        assert!(issues[0].repair.is_none());
        assert!(
            !report
                .issues
                .iter()
                .any(|issue| issue.kind == IssueKind::PossibleEnding)
        );
    }
    assert!(has(&tone(1., 1000., 0.0001), IssueKind::Silence));
    assert!(!has(&tone(1., 1000., 0.001), IssueKind::Silence));
}

#[test]
fn internal_quiet_gap_needs_substantial_audio_before_and_after() {
    let input = concatenate(&[tone(1.2, 1000., 0.1), silence(2.), tone(1.2, 1200., 0.1)]);
    let report = doctor::scan(&input, &|| false).unwrap();
    let endings: Vec<_> = report
        .issues
        .iter()
        .filter(|issue| issue.kind == IssueKind::PossibleEnding)
        .collect();
    assert_eq!(endings.len(), 1);
    assert_eq!(
        (endings[0].start_seconds, endings[0].end_seconds),
        (1.2, 3.2)
    );
    assert!(endings[0].repair.is_none());
    for parts in [
        vec![silence(2.), tone(1.2, 1000., 0.1)],
        vec![tone(1.2, 1000., 0.1), silence(2.)],
        vec![tone(0.1, 1000., 0.1), silence(2.), tone(1.2, 1000., 0.1)],
        vec![tone(1.2, 1000., 0.1), silence(2.), tone(0.1, 1000., 0.1)],
        vec![tone(1.2, 1000., 0.1), silence(1.4), tone(1.2, 1000., 0.1)],
        vec![tone(1.2, 1000., 0.1), silence(2.), tone(1.2, 1000., 0.003)],
    ] {
        assert!(!has(&concatenate(&parts), IssueKind::PossibleEnding));
    }
}

#[test]
fn pure_tones_and_sparse_tonal_controls_are_not_low_mid_buildup() {
    for frequency in [50., 220., 320., 450., 1000., 3500.] {
        assert!(!has(&tone(3., frequency, 0.25), IssueKind::LowMidBuildup));
    }
    let mut input = tone(3., 300., 0.2);
    let upper = tone(3., 2000., 0.06);
    for (sample, high) in input.samples.iter_mut().zip(upper.samples) {
        *sample += high;
    }
    assert!(!has(&input, IssueKind::LowMidBuildup));
}

#[test]
fn sustained_broad_low_mid_concentration_is_a_reviewable_measurement() {
    let input = broad_low_mids(3.);
    let report = doctor::scan(&input, &|| false).unwrap();
    let issue = report
        .issues
        .iter()
        .find(|issue| issue.kind == IssueKind::LowMidBuildup)
        .unwrap();
    assert!(
        issue.evidence["mean_low_mid_power_fraction"]
            .as_f64()
            .unwrap()
            > 0.55
    );
    assert_eq!(issue.evidence["audible_mud_confirmed"], false);
    assert_eq!(issue.repair, Some(doctor::RepairAction::ReduceLowMids));
    assert!(
        report.thresholds["low_mid_buildup"]["maximum_sampled_windows"]
            .as_u64()
            .unwrap()
            <= 1024
    );
}

#[test]
fn deterministic_white_noise_does_not_trigger_low_mid_buildup() {
    let mut state = 0x12345678_u32;
    let samples = (0..3 * 48000)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f64 / u32::MAX as f64 * 0.4 - 0.2) as f32
        })
        .collect();
    assert!(!has(
        &Audio::new(samples, 48000, 1).unwrap(),
        IssueKind::LowMidBuildup
    ));
}

#[test]
fn finding_cap_is_explicit_and_serialized_reports_are_deterministic() {
    let samples = [0.2, 0.5, 0.8, 1., 1., 1., 0.8, 0.5, 0.2].repeat(1000);
    let input = Audio::new(samples, 48000, 1).unwrap();
    let a = doctor::scan(&input, &|| false).unwrap();
    let b = doctor::scan(&input, &|| false).unwrap();
    assert_eq!(a.issues.len(), 512);
    assert!(a.truncated);
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
    assert_eq!(a.schema_version, 1);
    assert!(a.notes.iter().any(|note| note.contains("capped")));
}

#[test]
fn malformed_layout_and_nonfinite_samples_are_rejected() {
    for input in [
        Audio {
            samples: vec![],
            sample_rate: 48000,
            channels: 1,
        },
        Audio {
            samples: vec![0.; 3],
            sample_rate: 48000,
            channels: 2,
        },
        Audio {
            samples: vec![0.; 4],
            sample_rate: 48000,
            channels: 0,
        },
        Audio {
            samples: vec![0.; 6],
            sample_rate: 48000,
            channels: 3,
        },
        Audio {
            samples: vec![0.],
            sample_rate: 7999,
            channels: 1,
        },
        Audio {
            samples: vec![0.],
            sample_rate: 384001,
            channels: 1,
        },
        Audio {
            samples: vec![f32::NAN],
            sample_rate: 48000,
            channels: 1,
        },
        Audio {
            samples: vec![f32::INFINITY],
            sample_rate: 48000,
            channels: 1,
        },
    ] {
        assert!(doctor::scan(&input, &|| false).is_err());
    }
}

#[test]
fn cancellation_is_checked_before_validation_and_during_sample_walk() {
    let malformed = Audio {
        samples: vec![],
        sample_rate: 0,
        channels: 0,
    };
    assert_eq!(
        doctor::scan(&malformed, &|| true).unwrap_err().to_string(),
        "Cancelled"
    );
    let input = tone(3., 1000., 0.1);
    let calls = Cell::new(0);
    let error = doctor::scan(&input, &|| {
        calls.set(calls.get() + 1);
        calls.get() == 45
    })
    .unwrap_err();
    assert_eq!(error.to_string(), "Cancelled");
    assert_eq!(calls.get(), 45);
}
