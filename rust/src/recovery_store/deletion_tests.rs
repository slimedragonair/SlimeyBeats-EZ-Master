use super::tests::{ChildGuard, sample};
use super::*;
use std::{
    fs,
    time::{Duration, Instant},
};

fn child(root: &std::path::Path, stage: &str, id: Option<&str>) -> ChildGuard {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "recovery_store::tests::child_process_worker",
            "--nocapture",
        ])
        .env("SLIMEY_RECOVERY_TEST_ROOT", root)
        .env("SLIMEY_RECOVERY_TEST_HOLD_DELETE", stage);
    if let Some(id) = id {
        command.env("SLIMEY_RECOVERY_TEST_DELETE_RUN", id);
    } else {
        command.env("SLIMEY_RECOVERY_TEST_GC", "1");
    }
    let mut child = ChildGuard(command.spawn().unwrap());
    let start = Instant::now();
    while !root.parent().unwrap().join("ready").exists() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("deletion worker exited early: {status}");
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "deletion worker did not reach the failure point"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    child
}
#[test]
fn inspect_validates_metadata_and_live_lock_without_opening_pcm() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let mut store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let saved = store.save(&sample(), &|| false).unwrap();
    assert!(inspect(&root, &saved.checkpoint, &|| false).is_err());
    drop(store);
    let blob = fs::read_dir(root.join("blobs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::remove_file(blob.join("audio.wav")).unwrap();
    assert_eq!(
        inspect(&root, &saved.checkpoint, &|| false).unwrap(),
        sample().payload
    );
    assert!(load(&root, &saved.checkpoint, &|| false).is_err());
    let path = root
        .join("runs")
        .join(saved.checkpoint.run_id())
        .join("checkpoints")
        .join(manifest::filename(1));
    let mut record: serde_json::Value = filesystem::read_json(&path, MAX_METADATA_BYTES).unwrap();
    let changed = record["data_json"]
        .as_str()
        .unwrap()
        .replace("private-local-name", "changed");
    record["data_json"] = serde_json::json!(changed);
    fs::write(path, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(inspect(&root, &saved.checkpoint, &|| false).is_err());
    assert!(inspect(&root, &saved.checkpoint, &|| true).is_err());
}
#[test]
fn killed_run_deletion_resumes_without_harming_other_history() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let mut first = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let discarded = first.save(&sample(), &|| false).unwrap();
    first.save(&sample(), &|| false).unwrap();
    drop(first);
    let mut second = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let survivor = second.save(&sample(), &|| false).unwrap();
    drop(second);
    let mut worker = child(&root, "run", Some(discarded.checkpoint.run_id()));
    worker.kill().unwrap();
    worker.wait().unwrap();
    let mut reopened = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    assert_eq!(
        load(&root, &survivor.checkpoint, &|| false)
            .unwrap()
            .payload,
        sample().payload
    );
    assert!(
        !root
            .join("runs")
            .join(format!("discarded-{}", discarded.checkpoint.run_id()))
            .exists()
    );
    assert!(load(&root, &discarded.checkpoint, &|| false).is_err());
    reopened.finish(&|| false).unwrap();
    drop(reopened);
    assert_eq!(
        list_candidates(&root, &|| false).unwrap().checkpoints.len(),
        1
    );
}
#[test]
fn killed_orphan_blob_deletion_resumes_and_releases_global_lock() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let mut store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let saved = store.save(&sample(), &|| false).unwrap();
    let mut other = sample();
    other.audio = vec![std::sync::Arc::new(
        crate::audio::Audio::new(vec![0.2; 100], 48000, 2).unwrap(),
    )];
    let pending = root
        .join("runs")
        .join(store.run_id())
        .join("pending/checkpoint.json");
    assert!(store.save(&other, &|| pending.exists()).is_err());
    drop(store);
    let mut worker = child(&root, "blob", None);
    worker.kill().unwrap();
    worker.wait().unwrap();
    let mut reopened = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    assert_eq!(fs::read_dir(root.join("blobs")).unwrap().count(), 1);
    assert_eq!(
        load(&root, &saved.checkpoint, &|| false).unwrap().payload,
        sample().payload
    );
    reopened.finish(&|| false).unwrap();
}
#[test]
fn repeated_empty_and_clean_startups_do_not_exhaust_the_run_limit() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    for _ in 0..MAX_RUNS + 3 {
        let mut store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
        store.finish(&|| false).unwrap();
    }
    assert_eq!(fs::read_dir(root.join("runs")).unwrap().count(), 0);
    // Drop without finish simulates startup-only interruption; next open removes
    // only that empty owned scaffold, with no committed user history to discard.
    for _ in 0..MAX_RUNS + 3 {
        drop(Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap());
    }
    let mut final_store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    final_store.finish(&|| false).unwrap();
    assert!(
        list_candidates(&root, &|| false)
            .unwrap()
            .checkpoints
            .is_empty()
    );
}

#[test]
fn malformed_or_unknown_discarded_material_is_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let mut store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let saved = store.save(&sample(), &|| false).unwrap();
    drop(store);
    let original = root.join("runs").join(saved.checkpoint.run_id());
    let trash = root
        .join("runs")
        .join(format!("discarded-{}", saved.checkpoint.run_id()));
    fs::rename(original, &trash).unwrap();
    let checkpoint = trash.join("checkpoints").join(manifest::filename(1));
    fs::write(&checkpoint, b"{invalid").unwrap();
    assert!(Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).is_err());
    assert_eq!(fs::read(&checkpoint).unwrap(), b"{invalid");
    assert!(trash.join("run.json").exists());
    // A malformed discarded blob is not treated as permission to remove any of
    // its remaining audio, even though the root and generic owner marker match.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let mut store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    store.save(&sample(), &|| false).unwrap();
    drop(store);
    let blob = fs::read_dir(root.join("blobs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let name = blob.file_name().unwrap().to_str().unwrap().to_owned();
    let trash = root.join("blobs").join(format!("discarded-{name}"));
    fs::rename(blob, &trash).unwrap();
    fs::write(trash.join("blob.json"), b"{invalid").unwrap();
    fs::write(trash.join("personal.txt"), b"keep").unwrap();
    assert!(Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).is_err());
    assert_eq!(fs::read(trash.join("personal.txt")).unwrap(), b"keep");
    assert!(trash.join("audio.wav").exists());
}
