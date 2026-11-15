use super::*;
use crate::audio::Audio;
use serde_json::json;
use std::{fs, sync::Arc};

#[cfg(unix)]
#[test]
fn owned_private_root_is_restricted_to_its_user() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    drop(store);
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    let _reopened = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn complete_orphan_blob_from_cancelled_commit_is_reclaimed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let mut store = Store::open(&root, 16 * 1024, &|| false).unwrap();
    let old = Snapshot {
        payload: json!({"old":true}),
        audio: vec![Arc::new(Audio::new(vec![0.1; 100], 48000, 2).unwrap())],
    };
    let first = store.save(&old, &|| false).unwrap();
    let pending = root
        .join("runs")
        .join(store.run_id())
        .join("pending/checkpoint.json");
    let changed = Snapshot {
        payload: json!({"new":true}),
        audio: vec![Arc::new(Audio::new(vec![0.2; 1000], 48000, 2).unwrap())],
    };
    assert!(store.save(&changed, &|| pending.exists()).is_err());
    assert_eq!(fs::read_dir(root.join("blobs")).unwrap().count(), 2);
    store.save(&old, &|| false).unwrap();
    assert_eq!(fs::read_dir(root.join("blobs")).unwrap().count(), 1);
    store.finish(&|| false).unwrap();
    drop(store);
    assert_eq!(
        load(&root, &first.checkpoint, &|| false).unwrap().payload,
        old.payload
    );
    assert!(
        list_candidates(&root, &|| false)
            .unwrap()
            .checkpoints
            .is_empty()
    );
}

#[test]
fn duplicate_content_metadata_cannot_amplify_the_decoded_budget() {
    let audio = manifest::BlobRef {
        sha256: "0".repeat(64),
        bytes: MAX_ASSET_BYTES,
        sample_rate: 48000,
        channels: 1,
        samples: (MAX_ASSET_BYTES - 44) / 4,
    };
    audio.validate().unwrap();
    let data = manifest::Data {
        run_id: "run-amplification".into(),
        generation: 1,
        captured_unix_ms: 0,
        payload: json!({}),
        audio: vec![audio; 17],
    };
    let checkpoint = manifest::Checkpoint::new(data).unwrap();
    let reference = CheckpointRef {
        run_id: "run-amplification".into(),
        generation: 1,
    };
    let error = checkpoint.validate(&reference).unwrap_err();
    assert!(error.to_string().contains("decoded attachments exceed"));
}
#[test]
fn owned_partial_initialization_and_empty_clean_runs_do_not_accumulate() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let mut first = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    first.finish(&|| false).unwrap();
    assert_eq!(fs::read_dir(root.join("runs")).unwrap().count(), 0);
    drop(first);
    let id = "run-interrupted-initialization";
    let run = root.join("runs").join(id);
    fs::create_dir(&run).unwrap();
    filesystem::create_json(
        &run.join("run.json"),
        &manifest::Run {
            owner: manifest::Owner::new(),
            run_id: id.into(),
            created_unix_ms: 0,
        },
    )
    .unwrap();
    let store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    assert!(!run.exists());
    assert_eq!(fs::read_dir(root.join("runs")).unwrap().count(), 1);
    drop(store);
}
#[test]
fn fresh_store_refuses_to_reuse_corrupted_existing_audio() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let snapshot = Snapshot {
        payload: json!({}),
        audio: vec![Arc::new(Audio::new(vec![0.1; 100], 48000, 2).unwrap())],
    };
    let mut first = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    first.save(&snapshot, &|| false).unwrap();
    drop(first);
    let blob = fs::read_dir(root.join("blobs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("audio.wav");
    let mut bytes = fs::read(&blob).unwrap();
    bytes[48] ^= 1;
    fs::write(&blob, &bytes).unwrap();
    let mut second = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    assert!(
        second
            .save(&snapshot, &|| false)
            .unwrap_err()
            .to_string()
            .contains("checksum mismatch")
    );
    assert_eq!(fs::read(blob).unwrap(), bytes);
}
#[test]
fn false_owner_marker_and_unmarked_partial_material_are_not_adopted() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let id = store.run_id().to_owned();
    drop(store);
    let pending = root.join("runs").join(&id).join("pending");
    fs::remove_file(pending.join("owner.json")).unwrap();
    fs::write(pending.join("user-stuff.wav"), b"keep").unwrap();
    assert!(Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).is_err());
    assert_eq!(fs::read(pending.join("user-stuff.wav")).unwrap(), b"keep");
    let mut marker: serde_json::Value =
        filesystem::read_json(&root.join("owner.json"), 4096).unwrap();
    marker["owner"]["kind"] = json!("someone-elses-cache");
    fs::write(
        root.join("owner.json"),
        serde_json::to_vec(&marker).unwrap(),
    )
    .unwrap();
    assert!(list_candidates(&root, &|| false).is_err());
}
#[test]
fn payload_checksum_and_nonfinite_sample_inputs_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let mut store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let snapshot = Snapshot {
        payload: json!({"gain":0}),
        audio: Vec::new(),
    };
    let saved = store.save(&snapshot, &|| false).unwrap();
    let bad = Snapshot {
        payload: json!({}),
        audio: vec![Arc::new(Audio {
            samples: vec![f32::NAN],
            sample_rate: 48000,
            channels: 1,
        })],
    };
    assert!(store.save(&bad, &|| false).is_err());
    drop(store);
    let path = root
        .join("runs")
        .join(saved.checkpoint.run_id())
        .join("checkpoints")
        .join(manifest::filename(1));
    let mut value: serde_json::Value = filesystem::read_json(&path, MAX_METADATA_BYTES).unwrap();
    let changed = value["data_json"]
        .as_str()
        .unwrap()
        .replace("\"gain\":0", "\"gain\":12");
    value["data_json"] = json!(changed);
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    let listing = list_candidates(&root, &|| false).unwrap();
    assert!(listing.checkpoints.is_empty());
    assert_eq!(listing.warnings.len(), 1);
}
