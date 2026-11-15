use super::*;
use crate::config::Settings;
use std::f64::consts::TAU;

fn parameters(rate: u32, frequency: f64) -> EqParameters {
    let mut settings = Settings {
        eq_hold_loudness: false,
        ..Default::default()
    };
    settings.manual_eq[2].freq = frequency;
    settings.manual_eq[2].q = 18.;
    settings.manual_eq[2].gain = 6.;
    EqParameters::new(&settings, rate, 0.)
}

fn input(frame: usize, rate: u32) -> Frame {
    let t = frame as f64 / rate as f64;
    [0.04 * (TAU * 23. * t).sin(), 0.03 * (TAU * 61. * t).cos()]
}

#[test]
fn bounded_warmup_catches_up_with_the_exact_history_and_live_input() {
    for rate in [8000, 48000, 192000] {
        let mut eq = RealtimeEq::new(rate, 2, parameters(rate, 20.));
        let mut unchanged = RealtimeEq::new(rate, 2, parameters(rate, 20.));
        let initial = rate as usize * 2;
        for frame in 0..initial {
            eq.process(input(frame, rate));
            unchanged.process(input(frame, rate));
        }
        let target = parameters(rate, 22.);
        let mut reference = EqChain::new(target);
        // This target requires the full 1.5-second bounded history.
        let history = eq.history.len();
        for offset in 0..history {
            reference.process(eq.history[(eq.written + offset) % history], 2);
        }
        eq.set_parameters(target);
        let mut prepared_after = 0;
        for offset in 0..history {
            let frame = input(initial + offset, rate);
            let previous = eq.warming.as_ref().map(|w| w.remaining);
            let output = eq.process(frame);
            let unchanged_output = unchanged.process(frame);
            reference.process(frame, 2);
            if let Some(warmup) = &eq.warming {
                let before = previous.unwrap_or(history);
                assert_eq!(before - warmup.remaining, WARM_FRAMES_PER_SAMPLE - 1);
                assert_eq!(eq.current.parameters, parameters(rate, 20.));
                assert!(eq.next.is_none());
                assert_eq!(
                    output, unchanged_output,
                    "Preparation must preserve audible PCM"
                );
            } else {
                prepared_after = offset + 1;
                break;
            }
        }
        assert!(prepared_after > 1);
        assert!(prepared_after <= history.div_ceil(WARM_FRAMES_PER_SAMPLE - 1));
        let mut warmed = eq.next.expect("Prepared target should now be crossfading");
        for offset in 0..128 {
            let frame = input(initial + prepared_after + offset, rate);
            assert_eq!(warmed.process(frame, 2), reference.process(frame, 2));
        }
    }
}

#[test]
fn newest_target_survives_edits_during_preparation_and_fading() {
    let rate = 48000;
    let mut eq = RealtimeEq::new(rate, 2, parameters(rate, 20.));
    for frame in 0..rate as usize * 2 {
        eq.process(input(frame, rate));
    }
    eq.set_parameters(parameters(rate, 22.));
    eq.process(input(0, rate));
    assert!(eq.warming.is_some());
    let final_target = parameters(rate, 997.);
    let mut reference = EqChain::new(final_target);
    for frame in 0..rate as usize * 2 {
        if frame < 12000 && frame % 128 == 0 {
            eq.set_parameters(parameters(rate, 30. + frame as f64 / 128.));
        }
        if frame == 12000 {
            eq.set_parameters(final_target);
        }
        let source = input(frame, rate);
        assert!(eq.process(source).iter().all(|s| s.is_finite()));
        reference.process(source, 2);
    }
    assert_eq!(eq.current.parameters, final_target);
    assert!(eq.warming.is_none() && eq.next.is_none() && eq.pending.is_none());
    let mut actual = eq.current;
    for frame in 0..128 {
        let source = input(frame, rate);
        let difference = actual
            .process(source, 2)
            .into_iter()
            .zip(reference.process(source, 2))
            .map(|(a, b)| (a - b).abs())
            .fold(0_f64, f64::max);
        assert!(difference < 1e-10, "Final target error {difference}");
    }
}

#[test]
fn reset_discards_preparation_and_gain_only_edits_need_no_history_replay() {
    let rate = 48000;
    let original = parameters(rate, 20.);
    let mut eq = RealtimeEq::new(rate, 2, original);
    for frame in 0..rate as usize * 2 {
        eq.process(input(frame, rate));
    }
    let mut gain_only = original;
    gain_only.gain_db = -3.;
    eq.set_parameters(gain_only);
    eq.process(input(0, rate));
    assert!(eq.warming.is_none());
    assert!(eq.next.is_some());
    eq.reset(original);
    for frame in 0..10000 {
        eq.process(input(frame, rate));
    }
    eq.set_parameters(parameters(rate, 22.));
    eq.process(input(0, rate));
    assert!(eq.warming.is_some());
    eq.reset(original);
    let mut fresh = RealtimeEq::new(rate, 2, original);
    for frame in 0..10000 {
        let source = input(frame, rate);
        assert_eq!(eq.process(source), fresh.process(source));
    }
}
