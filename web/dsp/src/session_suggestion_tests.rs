use super::*;
use serde_json::{Value, json};
use std::f64::consts::TAU;

fn source(levels: [f64; 3]) -> Vec<f32> {
    (0..60000)
        .map(|frame| {
            [120., 1000., 14000.]
                .into_iter()
                .zip(levels)
                .map(|(hz, level)| level * (TAU * hz * frame as f64 / 48000.).sin())
                .sum::<f64>() as f32
        })
        .collect()
}

fn snapshot(session: &Session) -> Value {
    json!({
        "input": session.input.samples.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        "raw": session.raw.as_ref().map(|raw| raw.samples.iter().map(|x| x.to_bits()).collect::<Vec<_>>()),
        "report": session.report,
        "reference": session.reference,
        "stale": session.raw_reference_stale,
    })
}

fn check(session: &Session) {
    let before = snapshot(session);
    let detailed: Value = serde_json::from_str(&session.suggest_detailed("{}").unwrap()).unwrap();
    let legacy: Value = serde_json::from_str(&session.suggest("{}").unwrap()).unwrap();
    let measurement = analysis::analyze(
        session.raw.as_ref().unwrap_or(&session.input),
        &profile("neutral").unwrap(),
    )
    .unwrap();
    let proposal = eq_suggestion::proposal(&measurement, &profile("neutral").unwrap()).unwrap();
    assert_eq!(
        detailed,
        json!({"bands": proposal.bands, "notes": proposal.notes, "high_frequency": proposal.high_frequency})
    );
    assert_eq!(legacy, detailed["bands"]);
    assert!(legacy.is_array());
    assert_eq!(legacy.as_array().unwrap().len(), 7);
    assert_eq!(snapshot(session), before);
}

#[test]
fn detailed_and_legacy_suggestions_preserve_the_original_and_export_state() {
    let session = Session::new(source([0.04, 0.02, 0.001]), 48000, 1).unwrap();
    check(&session);
}

#[test]
fn detailed_and_legacy_suggestions_measure_the_unchanged_adopted_master() {
    let mut session = Session::new(source([0.04, 0.02, 0.001]), 48000, 1).unwrap();
    session
        .adopt_master(source([0.005, 0.02, 0.08]), "{}", "{}")
        .unwrap();
    check(&session);
}
