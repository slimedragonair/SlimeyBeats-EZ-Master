use mastercraft::{
    audio::Audio,
    config::Settings,
    dsp::{eq_chain::EqParameters, manual_eq, realtime_eq::RealtimeEq},
    meters,
};
use std::f64::consts::TAU;

fn tone(sr: u32, seconds: f64) -> Audio {
    Audio::new(
        (0..(sr as f64 * seconds) as usize)
            .flat_map(|i| {
                let t = i as f64 / sr as f64;
                vec![(0.12 * (TAU * 997. * t).sin() + 0.08 * (TAU * 11003. * t).sin()) as f32; 2]
            })
            .collect(),
        sr,
        2,
    )
    .unwrap()
}
#[test]
fn streaming_flat_eq_preserves_every_sample_and_duration() {
    for sr in [22050, 48000, 96000, 192000] {
        let input = tone(sr, 0.15);
        let parameters = EqParameters::new(&Settings::default(), sr, 0.);
        let output = manual_eq::render_fixed(&input, parameters, &|| false).unwrap();
        assert_eq!(input.samples, output.samples, "{sr}");
    }
}

#[test]
fn compensation_status_matches_the_final_level_after_peak_protection() {
    let input = tone(48000, 1.);
    let mut settings = Settings::default();
    settings.manual_eq[3].freq = 997.;
    settings.manual_eq[3].gain = 6.;
    let held = manual_eq::render(&input, &settings, &|| false).unwrap();
    assert!(
        (meters::integrated(&held.audio).unwrap() - meters::integrated(&input).unwrap()).abs()
            <= 0.05
    );
    assert!(held.converged);

    let mut hot = input.clone();
    hot.gain(12.);
    settings.eq_output_db = 12.;
    let limited = manual_eq::render(&hot, &settings, &|| false).unwrap();
    let requested = meters::integrated(&hot).unwrap() + 12.;
    assert!(meters::integrated(&limited.audio).unwrap() < requested - 3.);
    assert!(!limited.converged);

    hot.gain(12.);
    let flat = manual_eq::render(&hot, &Settings::default(), &|| false).unwrap();
    assert!(!flat.converged);
    assert!(meters::measure(&flat.audio).unwrap().true_peak_dbtp <= -1.);
}
#[test]
fn repeated_parameter_changes_stay_finite_and_under_true_peak_ceiling() {
    let input = tone(48000, 2.);
    let mut settings = Settings {
        eq_hold_loudness: false,
        ..Default::default()
    };
    settings.manual_eq[3].freq = 997.;
    settings.manual_eq[3].gain = 18.;
    let mut processor = RealtimeEq::new(48000, 2, EqParameters::new(&settings, 48000, 0.));
    let delay = processor.delay();
    let mut samples = Vec::new();
    for frame in 0..input.frames() + delay {
        if frame.is_multiple_of(2048) {
            settings.manual_eq[3].freq = [45., 997., 11003.][(frame / 2048) % 3];
            settings.manual_eq[3].q = [0.1, 1., 18.][(frame / 2048) % 3];
            settings.eq_output_db = if (frame / 2048) % 2 == 0 { 12. } else { -12. };
            processor.set_parameters(EqParameters::new(&settings, 48000, 0.));
        }
        let source = if frame < input.frames() {
            [
                input.samples[frame * 2] as f64,
                input.samples[frame * 2 + 1] as f64,
            ]
        } else {
            [0.; 2]
        };
        let value = processor.process(source);
        assert!(value.iter().all(|v| v.is_finite()));
        if frame >= delay {
            samples.extend(value.map(|v| v as f32));
        }
    }
    let output = Audio::new(samples, 48000, 2).unwrap();
    let peak = meters::measure(&output).unwrap().true_peak_dbtp;
    assert!(peak <= -0.98, "true peak {peak}");
    assert_eq!(input.frames(), output.frames());
}
#[test]
fn gain_changes_ramp_instead_of_jumping() {
    let settings = Settings {
        eq_hold_loudness: false,
        ..Default::default()
    };
    let mut next = settings.clone();
    next.eq_output_db = 6.;
    let mut processor = RealtimeEq::new(48000, 1, EqParameters::new(&settings, 48000, 0.));
    for _ in 0..10000 {
        processor.process([0.01, 0.]);
    }
    processor.set_parameters(EqParameters::new(&next, 48000, 0.));
    let mut last = 0.01_f64;
    let mut jump = 0_f64;
    for _ in 0..10000 {
        let value = processor.process([0.01, 0.])[0];
        jump = jump.max((value - last).abs());
        last = value;
    }
    assert!(jump < 0.00001, "largest per-sample step {jump}");
    assert!((last - 0.01 * meters::lin(6.)).abs() < 1e-8);
}
#[test]
fn shared_renderer_holds_loudness_and_reproduces_fixed_live_state() {
    let input = tone(48000, 1.);
    let mut settings = Settings::default();
    settings.manual_eq[3].freq = 997.;
    settings.manual_eq[3].gain = 6.;
    let rendered = manual_eq::render(&input, &settings, &|| false).unwrap();
    assert!(
        (meters::integrated(&rendered.audio).unwrap() - meters::integrated(&input).unwrap()).abs()
            < 0.1
    );
    let live = manual_eq::render_fixed(&input, rendered.parameters, &|| false).unwrap();
    let difference = live
        .samples
        .iter()
        .zip(&rendered.audio.samples)
        .map(|(a, b)| (a - b).abs())
        .fold(0_f32, f32::max);
    assert!(difference < 1e-6, "live/export difference {difference}");
}
#[test]
fn streaming_renderer_honors_cancellation() {
    let input = tone(48000, 0.5);
    assert!(
        manual_eq::render_fixed(
            &input,
            EqParameters::new(&Settings::default(), 48000, 0.),
            &|| true
        )
        .is_err()
    );
}
