use super::*;
use crate::audio::Audio;
use serde_json::json;
use std::{fs, sync::Arc};

fn audio(value: f32) -> Arc<Audio> {
    Arc::new(Audio::new(vec![value; 200], 48000, 2).unwrap())
}
fn wavs(root: &std::path::Path) -> Vec<Vec<u8>> {
    let mut bytes = fs::read_dir(root.join("blobs"))
        .unwrap()
        .map(|entry| fs::read(entry.unwrap().path().join("audio.wav")).unwrap())
        .collect::<Vec<_>>();
    bytes.sort();
    bytes
}
#[test]
fn retry_after_run_removal_reclaims_orphan_and_preserves_live_window_audio() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let shared = audio(0.1);
    let mut first = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let removed = first
        .save(
            &Snapshot {
                payload: json!({"window":1}),
                audio: vec![shared.clone(), audio(0.2)],
            },
            &|| false,
        )
        .unwrap();
    drop(first);
    let mut other = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let retained = other
        .save(
            &Snapshot {
                payload: json!({"window":2}),
                audio: vec![shared, audio(0.3)],
            },
            &|| false,
        )
        .unwrap();
    let other_run = root.join("runs").join(other.run_id());
    let before = wavs(&root);
    assert_eq!(before.len(), 3);
    let removed_run = root.join("runs").join(removed.checkpoint.run_id());
    let error = discard_run(&root, removed.checkpoint.run_id(), &|| {
        !removed_run.exists()
    })
    .unwrap_err();
    assert!(format!("{error:#}").contains("cancelled"));
    assert!(
        !removed_run.exists(),
        "the metadata deletion already committed"
    );
    assert_eq!(
        wavs(&root),
        before,
        "cancelled GC leaves private orphan audio to retry"
    );
    cleanup_unused(&root, &|| false).unwrap();
    let after = wavs(&root);
    assert_eq!(after.len(), 2);
    assert!(after.iter().all(|wav| before.contains(wav)));
    assert!(other_run.exists());
    assert!(
        discard_run(&root, other.run_id(), &|| false).is_err(),
        "other window remains locked"
    );
    cleanup_unused(&root, &|| false).unwrap();
    assert_eq!(wavs(&root), after, "retry is idempotent");
    drop(other);
    let restored = load(&root, &retained.checkpoint, &|| false).unwrap();
    assert_eq!(restored.payload, json!({"window":2}));
    assert_eq!(restored.audio[0].samples, vec![0.1; 200]);
    assert_eq!(restored.audio[1].samples, vec![0.3; 200]);
}
#[test]
fn cleanup_retry_absent_root_is_noop_but_unknown_or_malformed_material_errors() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    cleanup_unused(&root, &|| false).unwrap();
    assert!(!root.exists());
    let mut store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let saved = store
        .save(
            &Snapshot {
                payload: json!({}),
                audio: vec![audio(0.1)],
            },
            &|| false,
        )
        .unwrap();
    drop(store);
    let run = root.join("runs").join(saved.checkpoint.run_id());
    assert!(discard_run(&root, saved.checkpoint.run_id(), &|| !run.exists()).is_err());
    let before = wavs(&root);
    let unknown = root.join("personal.wav");
    fs::write(&unknown, b"private unknown material").unwrap();
    assert!(cleanup_unused(&root, &|| false).is_err());
    assert_eq!(fs::read(&unknown).unwrap(), b"private unknown material");
    assert_eq!(wavs(&root), before);
    fs::remove_file(unknown).unwrap();
    let blob = fs::read_dir(root.join("blobs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(blob.join("blob.json"), b"{malformed").unwrap();
    assert!(cleanup_unused(&root, &|| false).is_err());
    assert_eq!(wavs(&root), before);
    assert_eq!(fs::read(blob.join("blob.json")).unwrap(), b"{malformed");
}

#[test]
fn repeat_discard_of_absent_run_is_idempotent_and_preserves_other_live_run() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let mut first = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let saved = first
        .save(
            &Snapshot {
                payload: json!({"window":1}),
                audio: vec![audio(0.1)],
            },
            &|| false,
        )
        .unwrap();
    drop(first);
    let mut other = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let retained = other
        .save(
            &Snapshot {
                payload: json!({"window":2}),
                audio: vec![audio(0.2)],
            },
            &|| false,
        )
        .unwrap();
    discard_run(&root, saved.checkpoint.run_id(), &|| false).unwrap();
    let expected = wavs(&root);
    assert_eq!(expected.len(), 1);
    discard_run(&root, saved.checkpoint.run_id(), &|| false).unwrap();
    cleanup_unused(&root, &|| false).unwrap();
    assert_eq!(wavs(&root), expected);
    assert!(root.join("runs").join(other.run_id()).exists());
    assert!(discard_run(&root, other.run_id(), &|| false).is_err());
    assert!(discard_run(&root, "../outside", &|| false).is_err());
    drop(other);
    assert_eq!(
        load(&root, &retained.checkpoint, &|| false).unwrap().audio[0].samples,
        vec![0.2; 200]
    );
}
