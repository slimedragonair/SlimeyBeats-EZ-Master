use super::*;
use serde_json::json;

#[test]
fn debounce_saves_latest_value_and_reopen_reads_unicode_path() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("settings with spaces.json");
    let original = json!({"out_dir": "old"});
    let mut store = PreferenceStore::new(path.clone(), &original, true);
    let now = Instant::now();
    store.update(&original, now, false, false).unwrap();
    assert!(
        !path.exists(),
        "Opening an app must not write session overrides"
    );
    let first = json!({"out_dir": "Exports/龍 mixes"});
    store.update(&first, now, false, false).unwrap();
    assert!(!path.exists());
    let final_value = json!({"out_dir": "Exports/龍 mixes/é"});
    store
        .update(&final_value, now + SAVE_DELAY / 2, false, false)
        .unwrap();
    store
        .update(&final_value, now + SAVE_DELAY, false, false)
        .unwrap();
    assert!(!path.exists(), "A continuing edit restarts the debounce");
    store
        .update(&final_value, now + SAVE_DELAY * 2, false, false)
        .unwrap();
    let reopened: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(reopened, final_value);
    let mut next_process = PreferenceStore::new(path.clone(), &reopened, true);
    next_process
        .update(&json!({"out_dir": "another"}), now, true, false)
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap()).unwrap()["out_dir"],
        "another"
    );
}

#[test]
fn close_flushes_the_last_edit_and_reverting_cancels_pending_save() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("prefs.json");
    let mut store = PreferenceStore::new(path.clone(), &0, true);
    let now = Instant::now();
    store.update(&1, now, false, false).unwrap();
    store.update(&0, now, false, false).unwrap();
    assert!(store.due_in(now).is_none());
    assert!(!path.exists());
    store.update(&2, now, false, false).unwrap();
    store.update(&2, now, true, false).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "2");
}

#[test]
fn corrupt_file_is_preserved_until_explicit_save() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("prefs.json");
    std::fs::write(&path, "broken private configuration").unwrap();
    let mut store = PreferenceStore::new(path.clone(), &0, false);
    let now = Instant::now();
    store.update(&1, now, false, false).unwrap();
    store.update(&1, now, true, false).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "broken private configuration"
    );
    store.update(&1, now, true, true).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "1");
    store.update(&2, now, true, false).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "2");
}

#[test]
fn failed_save_is_reported_once_and_can_retry_without_losing_the_old_file() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("prefs.json");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("preserved.txt"), "keep").unwrap();
    let mut store = PreferenceStore::new(path.clone(), &0, true);
    let now = Instant::now();
    assert!(store.update(&1, now, true, false).is_err());
    assert_eq!(
        std::fs::read_to_string(path.join("preserved.txt")).unwrap(),
        "keep"
    );
    assert!(store.update(&1, now, false, false).is_ok());
    assert!(store.due_in(now).is_none());
    assert!(store.update(&1, now, true, true).is_err());
    assert_eq!(
        std::fs::read_dir(folder.path()).unwrap().count(),
        1,
        "No abandoned temporary files"
    );
}
