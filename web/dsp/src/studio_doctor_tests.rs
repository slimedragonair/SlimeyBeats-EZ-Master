use super::*;
use serde_json::{Value, json};

fn info(studio: &BrowserStudio) -> Value {
    serde_json::from_str(&studio.info().unwrap()).unwrap()
}

#[test]
fn scan_is_read_only_and_doctor_keeps_origin_and_history() {
    let mut studio = BrowserStudio::new();
    let samples = (0..16000)
        .map(|i| 0.4 * (i as f32 * 0.196).sin())
        .collect::<Vec<_>>();
    studio.load(samples.clone(), 8000, 1).unwrap();
    let original = info(&studio);
    let report: Value = serde_json::from_str(&studio.scan_issues("source").unwrap()).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(info(&studio), original);
    assert_eq!(studio.source.as_ref().unwrap().samples, samples);
    let request = json!({"action":"reduce_low_mids", "start_seconds":0.25,
        "end_seconds":1.5, "strength":0.5})
    .to_string();
    studio.doctor_checked("source", &request).unwrap();
    let after = studio.current.as_ref().unwrap().samples.clone();
    assert_eq!(studio.source.as_ref().unwrap().samples, samples);
    assert_eq!(studio.before.as_ref().unwrap().samples, samples);
    assert!(!studio.preview_is_mix);
    studio.history(false).unwrap();
    assert_eq!(studio.current.as_ref().unwrap().samples, samples);
    studio.history(true).unwrap();
    assert_eq!(studio.current.as_ref().unwrap().samples, after);
    let snapshot = info(&studio);
    for bad in [
        "{",
        r#"{"action":"reduce_low_mids","start_seconds":-1,"end_seconds":1,"strength":0.5}"#,
        r#"{"action":"declip","start_seconds":0,"end_seconds":1,"strength":1.1}"#,
    ] {
        assert!(studio.doctor_checked("source", bad).is_err());
        assert_eq!(info(&studio), snapshot);
        assert_eq!(studio.current.as_ref().unwrap().samples, after);
    }
}

#[test]
fn doctor_stem_preview_requires_explicit_bank_replacement() {
    let mut studio = BrowserStudio::new();
    studio
        .stage_stem("vocals", vec![0.1; 16000], 8000, 1)
        .unwrap();
    studio
        .stage_stem("instrumental", vec![0.02; 16000], 8000, 1)
        .unwrap();
    studio.commit_stems().unwrap();
    let source = studio.source.clone().unwrap();
    let bank = studio.audio("vocals").unwrap();
    studio
        .doctor_checked(
            "vocals",
            &json!({"action":"reduce_low_mids",
        "start_seconds":0.2,"end_seconds":1.5,"strength":0.5})
            .to_string(),
        )
        .unwrap();
    assert_eq!(studio.direct_edit_stems(), vec!["vocals".parse().unwrap()]);
    assert!(std::sync::Arc::ptr_eq(
        &bank,
        &studio.audio("vocals").unwrap()
    ));
    studio.replace_stem("vocals").unwrap();
    assert!(studio.preview_is_mix);
    assert!(std::sync::Arc::ptr_eq(
        &source,
        studio.source.as_ref().unwrap()
    ));
    studio.history(false).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &bank,
        &studio.audio("vocals").unwrap()
    ));
}
