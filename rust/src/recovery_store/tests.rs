use super::*;
use crate::audio::Audio;
use serde_json::json;
use std::{
    fs,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;

pub(super) struct ChildGuard(pub(super) std::process::Child);
impl std::ops::Deref for ChildGuard {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for ChildGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn sound(n: usize, value: f32) -> Arc<Audio> {
    Arc::new(Audio::new(vec![value; n], 48000, 2).unwrap())
}
pub(super) fn sample() -> Snapshot {
    let audio = Arc::new(Audio::new(vec![0.0, -0.0, 0.1, -0.1, 1.2, -1.2], 48000, 2).unwrap());
    Snapshot {
        payload: json!({"version":1,"source":0,"direct_stem":0,"name":"private-local-name"}),
        audio: vec![audio.clone(), Arc::new((*audio).clone())],
    }
}
fn setup() -> (TempDir, std::path::PathBuf, Store) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let store = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    (temp, root, store)
}
#[test]
fn exact_float_samples_and_logical_attachment_identity_survive_reload() {
    let (_temp, root, mut store) = setup();
    let snapshot = sample();
    let saved = store.save(&snapshot, &|| false).unwrap();
    assert!(
        load(&root, &saved.checkpoint, &|| false).is_err(),
        "live run must not be loaded"
    );
    drop(store);
    let restored = load(&root, &saved.checkpoint, &|| false).unwrap();
    assert_eq!(restored.payload, snapshot.payload);
    assert!(!Arc::ptr_eq(&restored.audio[0], &restored.audio[1]));
    let direct_index = restored.payload["direct_stem"].as_u64().unwrap() as usize;
    let direct_stem = restored.audio[direct_index].clone();
    assert!(Arc::ptr_eq(&direct_stem, &restored.audio[0]));
    let bits = |a: &Audio| a.samples.iter().map(|s| s.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(&restored.audio[0]), bits(&snapshot.audio[0]));
    let names = filesystem::entries(&root.join("blobs"), MAX_BLOBS, &|| false).unwrap();
    let mut wav =
        hound::WavReader::open(root.join("blobs").join(&names[0]).join("audio.wav")).unwrap();
    assert_eq!(wav.spec().sample_format, hound::SampleFormat::Float);
    assert_eq!(
        wav.samples::<f32>()
            .map(|s| s.unwrap().to_bits())
            .collect::<Vec<_>>(),
        bits(&snapshot.audio[0])
    );
}
#[test]
fn reused_and_content_identical_attachments_are_not_rewritten() {
    let (_temp, root, mut store) = setup();
    let mut snapshot = sample();
    let first = store.save(&snapshot, &|| false).unwrap();
    let name = filesystem::entries(&root.join("blobs"), MAX_BLOBS, &|| false)
        .unwrap()
        .pop()
        .unwrap();
    let path = root.join("blobs").join(name).join("audio.wav");
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    snapshot.audio.push(Arc::new((*snapshot.audio[0]).clone()));
    for n in 0..8 {
        snapshot.payload["gain"] = json!(n);
        store.save(&snapshot, &|| false).unwrap();
    }
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    assert_eq!(
        filesystem::entries(&root.join("blobs"), MAX_BLOBS, &|| false)
            .unwrap()
            .len(),
        1
    );
    assert!(load(&root, &first.checkpoint, &|| false).is_err());
    drop(store);
    let points = list_candidates(&root, &|| false).unwrap();
    assert_eq!(points.checkpoints.len(), RETAINED_CHECKPOINTS);
    let restored = load(&root, &points.checkpoints[0].checkpoint, &|| false).unwrap();
    assert!(!Arc::ptr_eq(&restored.audio[0], &restored.audio[2]));
}
#[test]
fn two_windows_exclude_live_runs_and_preserve_each_others_points() {
    let (_temp, root, mut one) = setup();
    let mut two = Store::open(&root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    let first = one.save(&sample(), &|| false).unwrap();
    two.save(&sample(), &|| false).unwrap();
    assert!(
        list_candidates(&root, &|| false)
            .unwrap()
            .checkpoints
            .is_empty()
    );
    assert!(discard_run(&root, one.run_id(), &|| false).is_err());
    drop(one);
    assert_eq!(
        list_candidates(&root, &|| false).unwrap().checkpoints.len(),
        1
    );
    let recovered = load(&root, &first.checkpoint, &|| false).unwrap();
    two.save(&recovered, &|| false).unwrap();
    discard_run(&root, first.checkpoint.run_id(), &|| false).unwrap();
    assert_eq!(
        fs::read_dir(root.join("blobs")).unwrap().count(),
        1,
        "other live window still references this blob"
    );
    two.finish(&|| false).unwrap();
    drop(two);
    assert!(
        list_candidates(&root, &|| false)
            .unwrap()
            .checkpoints
            .is_empty()
    );
    assert_eq!(list_history(&root, &|| false).unwrap().checkpoints.len(), 2);
}
#[test]
fn shared_budget_refusal_retains_committed_work() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let budget = 16 * 1024;
    let mut one = Store::open(&root, budget, &|| false).unwrap();
    let first = one
        .save(
            &Snapshot {
                payload: json!({"old":true}),
                audio: vec![sound(2000, 0.1)],
            },
            &|| false,
        )
        .unwrap();
    let mut two = Store::open(&root, budget, &|| false).unwrap();
    let error = two
        .save(
            &Snapshot {
                payload: json!({}),
                audio: vec![sound(2000, 0.2)],
            },
            &|| false,
        )
        .unwrap_err();
    assert!(error.to_string().contains("full"));
    assert!(
        Store::open(&root, budget * 2, &|| false).is_err(),
        "cannot bypass persisted global quota"
    );
    assert!(cleanup::inventory(&root, &|| false).unwrap().0 <= budget);
    drop(one);
    assert_eq!(
        load(&root, &first.checkpoint, &|| false).unwrap().payload,
        json!({"old":true})
    );
}
#[test]
fn disk_full_injection_keeps_old_commit_and_next_save_recovers() {
    let (_temp, root, mut store) = setup();
    let first = store.save(&sample(), &|| false).unwrap();
    filesystem::FAIL_NEXT_WRITE.with(|cell| cell.set(Some(32)));
    assert!(store.save(&sample(), &|| false).is_err());
    assert_eq!(
        fs::read_dir(root.join("runs").join(store.run_id()).join("checkpoints"))
            .unwrap()
            .count(),
        1
    );
    let second = store.save(&sample(), &|| false).unwrap();
    assert_eq!(second.checkpoint.generation(), 2);
    drop(store);
    assert!(load(&root, &first.checkpoint, &|| false).is_ok());
    assert!(load(&root, &second.checkpoint, &|| false).is_ok());
}
#[test]
fn cancellation_mid_audio_write_preserves_prior_checkpoint() {
    let (_temp, root, mut store) = setup();
    let first = store.save(&sample(), &|| false).unwrap();
    let pending_audio = root
        .join("runs")
        .join(store.run_id())
        .join("pending/blob/audio.wav");
    let cancelled = || fs::metadata(&pending_audio).is_ok_and(|m| m.len() > 128 * 1024);
    let large = Snapshot {
        payload: json!({"new":true}),
        audio: vec![sound(1024 * 1024, 0.2)],
    };
    assert!(store.save(&large, &cancelled).is_err());
    store.save(&sample(), &|| false).unwrap();
    drop(store);
    assert!(load(&root, &first.checkpoint, &|| false).is_ok());
    assert_eq!(fs::read_dir(root.join("blobs")).unwrap().count(), 1);
}
#[test]
fn corrupt_generation_does_not_hide_older_intact_checkpoint() {
    let (_temp, root, mut store) = setup();
    let first = store.save(&sample(), &|| false).unwrap();
    let next = root
        .join("runs")
        .join(store.run_id())
        .join("checkpoints")
        .join(manifest::filename(2));
    fs::write(&next, b"{truncated").unwrap();
    drop(store);
    let listing = list_candidates(&root, &|| false).unwrap();
    assert_eq!(listing.checkpoints.len(), 1);
    assert_eq!(listing.warnings.len(), 1);
    assert!(load(&root, &first.checkpoint, &|| false).is_ok());
    assert!(discard_run(&root, first.checkpoint.run_id(), &|| false).is_err());
    assert_eq!(fs::read(next).unwrap(), b"{truncated");
}
#[test]
fn invalid_owner_traversal_links_and_unknown_files_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let unowned = temp.path().join("unowned");
    fs::create_dir(&unowned).unwrap();
    fs::write(unowned.join("keep.txt"), b"user").unwrap();
    assert!(Store::open(&unowned, DEFAULT_BUDGET_BYTES, &|| false).is_err());
    let (_owned_temp, root, mut store) = setup();
    let point = store.save(&sample(), &|| false).unwrap();
    let bad: CheckpointRef =
        serde_json::from_value(json!({"run_id":"../escape","generation":1})).unwrap();
    assert!(load(&root, &bad, &|| false).is_err());
    let unknown = root.join("runs").join(store.run_id()).join("precious.wav");
    fs::write(&unknown, b"user-owned").unwrap();
    drop(store);
    assert!(discard_run(&root, point.checkpoint.run_id(), &|| false).is_err());
    assert_eq!(fs::read(unknown).unwrap(), b"user-owned");
    let link = temp.path().join("linked-owner.json");
    fs::hard_link(root.join("owner.json"), &link).unwrap();
    assert!(list_candidates(&root, &|| false).is_err());
    assert_eq!(fs::read(unowned.join("keep.txt")).unwrap(), b"user");
}
#[test]
fn checksum_and_layout_damage_are_rejected_without_touching_source() {
    let (_temp, root, mut store) = setup();
    let point = store.save(&sample(), &|| false).unwrap();
    drop(store);
    let name = fs::read_dir(root.join("blobs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let wav = name.join("audio.wav");
    let mut bytes = fs::read(&wav).unwrap();
    bytes[48] ^= 1;
    fs::write(&wav, &bytes).unwrap();
    assert!(load(&root, &point.checkpoint, &|| false).is_err());
    assert_eq!(fs::read(&wav).unwrap(), bytes);
    assert_eq!(
        list_candidates(&root, &|| false).unwrap().checkpoints.len(),
        1,
        "listing doesn't read PCM"
    );
}
#[test]
fn bounds_and_cancelled_lock_wait_are_explicit() {
    let (_temp, root, mut store) = setup();
    let mut snapshot = sample();
    snapshot.audio = vec![snapshot.audio[0].clone(); MAX_AUDIO_ATTACHMENTS + 1];
    assert!(store.save(&snapshot, &|| false).is_err());
    snapshot = sample();
    snapshot.payload = json!("x".repeat(MAX_METADATA_BYTES as usize));
    assert!(store.save(&snapshot, &|| false).is_err());
    let _held = filesystem::lock(&root.join("store.lock"), &|| false).unwrap();
    let start = Instant::now();
    assert!(list_candidates(&root, &|| start.elapsed() > Duration::from_millis(50)).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
}

// A real child process holds the OS lock until killed. This fixture also verifies
// fresh-process disk loading, so tests don't depend on process-local caches.
#[test]
fn child_process_worker() {
    let Some(root) = std::env::var_os("SLIMEY_RECOVERY_TEST_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    if let Some(id) = std::env::var_os("SLIMEY_RECOVERY_TEST_DELETE_RUN") {
        discard_run(root, id.to_str().unwrap(), &|| false).unwrap();
        return;
    }
    if std::env::var_os("SLIMEY_RECOVERY_TEST_GC").is_some() {
        let mut store = Store::open(root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
        store.save(&sample(), &|| false).unwrap();
        return;
    }
    if std::env::var_os("SLIMEY_RECOVERY_TEST_LOAD").is_some() {
        let listing = list_candidates(root, &|| false).unwrap();
        assert_eq!(listing.checkpoints.len(), 1);
        let snapshot = load(root, &listing.checkpoints[0].checkpoint, &|| false).unwrap();
        assert_eq!(snapshot.payload, sample().payload);
        assert!(!Arc::ptr_eq(&snapshot.audio[0], &snapshot.audio[1]));
        return;
    }
    let mut store = Store::open(root, DEFAULT_BUDGET_BYTES, &|| false).unwrap();
    store.save(&sample(), &|| false).unwrap();
    fs::write(root.parent().unwrap().join("ready"), b"ready").unwrap();
    loop {
        std::thread::sleep(Duration::from_millis(100));
    }
}
#[test]
fn killed_process_releases_lock_and_fresh_process_recovers() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("recovery");
    let mut child = ChildGuard(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "recovery_store::tests::child_process_worker",
                "--nocapture",
            ])
            .env("SLIMEY_RECOVERY_TEST_ROOT", &root)
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    while !temp.path().join("ready").exists() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("child exited early: {status}");
        }
        if start.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            panic!("child readiness timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        list_candidates(&root, &|| false)
            .unwrap()
            .checkpoints
            .is_empty()
    );
    child.kill().unwrap();
    child.wait().unwrap();
    let listing = list_candidates(&root, &|| false).unwrap();
    assert_eq!(listing.checkpoints.len(), 1);
    assert!(listing.checkpoints[0].interrupted);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "recovery_store::tests::child_process_worker",
            "--nocapture",
        ])
        .env("SLIMEY_RECOVERY_TEST_ROOT", &root)
        .env("SLIMEY_RECOVERY_TEST_LOAD", "1")
        .status()
        .unwrap();
    assert!(status.success());
}
