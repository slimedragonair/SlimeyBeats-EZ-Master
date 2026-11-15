use super::*;
use serde_json::json;
use std::fs;

fn float_payload() -> serde_json::Value {
    let mut values = vec![
        f64::MAX,
        -f64::MAX,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::from_bits(0x000f_ffff_ffff_ffff),
        -0.0,
        0.0,
        0.1,
        std::f64::consts::PI,
        0.21608395558577298,
    ];
    let mut state = 0x8727_54ab_3cdf_1029_u64;
    for _ in 0..1024 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let number = f64::from_bits(state);
        if number.is_finite() {
            values.push(number);
        }
    }
    json!({"analysis":{"loudness":values,"nested":[{"controls":[-0.0,0.83,0.27]}]},"version":1})
}
#[test]
fn arbitrary_nested_f64_wire_checksum_survives_parse_and_retention() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let payload = float_payload();
    let mut store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let mut last = None;
    for _ in 0..5 {
        let point = store
            .save(
                &Snapshot {
                    payload: payload.clone(),
                    audio: vec![],
                },
                &|| false,
            )
            .unwrap();
        assert!(
            point.maintenance_warning.is_none(),
            "{:?}",
            point.maintenance_warning
        );
        last = Some(point.checkpoint);
    }
    store.finish(&|| false).unwrap();
    drop(store);
    let listing = list_history(&root, &|| false).unwrap();
    assert_eq!(listing.checkpoints.len(), RETAINED_CHECKPOINTS);
    assert!(listing.warnings.is_empty(), "{:?}", listing.warnings);
    let restored = load(&root, last.as_ref().unwrap(), &|| false).unwrap();
    assert_eq!(
        restored.payload,
        inspect(&root, last.as_ref().unwrap(), &|| false).unwrap()
    );
    assert_eq!(
        restored.payload["analysis"]["nested"][0]["controls"][0]
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );
    // With default serde parsing the numeric Value can legitimately differ by
    // one ULP. The exact on-disk data string must still verify, without relying
    // on float_roundtrip being enabled by any other dependency in the build.
    for point in &listing.checkpoints {
        let path = root
            .join("runs")
            .join(point.checkpoint.run_id())
            .join("checkpoints")
            .join(manifest::filename(point.checkpoint.generation()));
        let envelope: serde_json::Value = filesystem::read_json(&path, MAX_METADATA_BYTES).unwrap();
        let wire = envelope["data_json"].as_str().unwrap();
        assert_eq!(
            envelope["sha256"].as_str().unwrap(),
            manifest::hash(wire.as_bytes())
        );
        let expected = manifest::Data {
            run_id: point.checkpoint.run_id().into(),
            generation: point.checkpoint.generation(),
            captured_unix_ms: point.captured_unix_ms,
            payload: payload.clone(),
            audio: vec![],
        };
        assert_eq!(
            wire.as_bytes(),
            filesystem::json_bytes(&expected, MAX_METADATA_BYTES).unwrap()
        );
    }
}
#[test]
fn float_payload_tampering_is_rejected_before_data_json_is_parsed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let mut store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let saved = store
        .save(
            &Snapshot {
                payload: float_payload(),
                audio: vec![],
            },
            &|| false,
        )
        .unwrap();
    drop(store);
    let path = root
        .join("runs")
        .join(saved.checkpoint.run_id())
        .join("checkpoints")
        .join(manifest::filename(saved.checkpoint.generation()));
    let mut envelope: serde_json::Value = filesystem::read_json(&path, MAX_METADATA_BYTES).unwrap();
    envelope["data_json"] = json!("{not valid JSON");
    fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    let error = inspect(&root, &saved.checkpoint, &|| false).unwrap_err();
    assert!(format!("{error:#}").contains("Recovery checkpoint checksum mismatch"));
}
