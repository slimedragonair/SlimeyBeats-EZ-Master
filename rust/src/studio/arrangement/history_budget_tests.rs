use super::{ArrangementHistory, DEFAULT_HISTORY_METADATA_BYTES, HistorySnapshot};
use crate::studio::arrangement::{Arrangement, AssetLibrary, Edit, Lane, MAX_METADATA_BYTES};

fn document(name: &str) -> Arrangement {
    let mut document = Arrangement::new(8000, 1);
    document.lanes.push(Lane::new("track", name));
    document
}
fn rename(name: &str) -> Edit {
    Edit::SetLane {
        lane_id: "track".into(),
        name: Some(name.into()),
        gain_db: None,
        pan: None,
        mute: None,
        solo: None,
    }
}
fn exact_size(history: &ArrangementHistory) {
    assert_eq!(
        history.metadata_bytes(),
        serde_json::to_vec(&history.snapshot()).unwrap().len()
    );
    assert!(history.metadata_bytes() <= history.metadata_budget_bytes());
}

#[test]
fn cached_counts_match_real_json_through_edits_branching_undo_and_redo() {
    let assets = AssetLibrary::new();
    let mut history = ArrangementHistory::new(document("Initial"), &assets, 8).unwrap();
    for index in 0..24 {
        history
            .apply(&rename(&format!("Track λ \"quoted\" {index}")), &assets)
            .unwrap();
        exact_size(&history);
    }
    assert_eq!(history.undo_len(), 8);
    for _ in 0..4 {
        history.undo().unwrap();
        exact_size(&history);
    }
    for _ in 0..2 {
        history.redo().unwrap();
        exact_size(&history);
    }
    history.apply(&rename("New branch"), &assets).unwrap();
    assert!(!history.can_redo());
    exact_size(&history);
}

#[test]
fn byte_eviction_preserves_current_and_nearest_undo_without_reserializing_old_states() {
    let assets = AssetLibrary::new();
    let first = document(&"a".repeat(500));
    let snapshot = HistorySnapshot {
        current: first.clone(),
        undo: vec![first.clone()],
        redo: vec![],
        capacity: 64,
    };
    let budget = serde_json::to_vec(&snapshot).unwrap().len();
    let mut history = ArrangementHistory::new_with_budget(first, &assets, 64, budget).unwrap();
    for letter in ["b", "c", "d", "e"] {
        history
            .apply(&rename(&letter.repeat(500)), &assets)
            .unwrap();
        exact_size(&history);
        assert_eq!(history.undo_len(), 1);
    }
    assert_eq!(history.current().lanes[0].name, "e".repeat(500));
    assert_eq!(history.undo().unwrap().lanes[0].name, "d".repeat(500));
    assert!(!history.can_undo());
    exact_size(&history);
    assert_eq!(history.redo().unwrap().lanes[0].name, "e".repeat(500));
    exact_size(&history);
}

#[test]
fn exact_envelope_budget_evicts_farthest_redo_when_array_punctuation_grows() {
    let assets = AssetLibrary::new();
    let snapshot = HistorySnapshot {
        current: document("b"),
        undo: vec![document("a")],
        redo: vec![document("c")],
        capacity: 64,
    };
    let budget = serde_json::to_vec(&snapshot).unwrap().len();
    let mut history =
        ArrangementHistory::from_snapshot_with_budget(snapshot, &assets, budget).unwrap();
    assert_eq!(history.metadata_bytes(), budget);
    assert_eq!(history.undo().unwrap().lanes[0].name, "a");
    assert_eq!(history.redo_len(), 1);
    assert_eq!(history.redo().unwrap().lanes[0].name, "b");
    assert!(!history.can_redo());
    exact_size(&history);
}

#[test]
fn failed_admission_and_too_large_current_leave_caches_and_redo_unchanged() {
    let assets = AssetLibrary::new();
    let initial = document("a");
    let budget = serde_json::to_vec(&HistorySnapshot {
        current: initial.clone(),
        undo: vec![],
        redo: vec![],
        capacity: 8,
    })
    .unwrap()
    .len();
    let mut history = ArrangementHistory::new_with_budget(initial, &assets, 8, budget).unwrap();
    let before = serde_json::to_value(history.snapshot()).unwrap();
    assert!(history.apply(&rename("larger than a"), &assets).is_err());
    assert_eq!(serde_json::to_value(history.snapshot()).unwrap(), before);
    exact_size(&history);
    let mut history = ArrangementHistory::new(document("a"), &assets, 8).unwrap();
    history.apply(&rename("b"), &assets).unwrap();
    history.undo().unwrap();
    let before = serde_json::to_value(history.snapshot()).unwrap();
    let bytes = history.metadata_bytes();
    assert!(
        history
            .apply_batch_checked(&[rename("c")], &assets, |_| anyhow::bail!(
                "Rejected envelope"
            ))
            .is_err()
    );
    assert_eq!(serde_json::to_value(history.snapshot()).unwrap(), before);
    assert_eq!(history.metadata_bytes(), bytes);
    assert!(history.can_redo());
}

#[test]
fn default_restoration_allows_history_over_sixteen_mib_and_evicts_at_thirty_two() {
    let assets = AssetLibrary::new();
    let mut large = Arrangement::new(8000, 1);
    let name = "x".repeat(500);
    large.lanes = (0..13_000)
        .map(|index| Lane::new(format!("lane-{index}"), name.clone()))
        .collect();
    large.lanes[0].id = "track".into();
    let snapshot = HistorySnapshot {
        current: large.clone(),
        undo: vec![large.clone(), large],
        redo: vec![],
        capacity: 64,
    };
    let size = serde_json::to_vec(&snapshot).unwrap().len();
    assert!(size > MAX_METADATA_BYTES && size < DEFAULT_HISTORY_METADATA_BYTES);
    let mut history = ArrangementHistory::from_snapshot(snapshot, &assets).unwrap();
    assert_eq!(history.metadata_bytes(), size);
    for letter in ["a", "b", "c"] {
        history
            .apply(&rename(&letter.repeat(500)), &assets)
            .unwrap();
    }
    assert!(history.undo_len() < 5);
    assert_eq!(history.current().lanes[0].name, "c".repeat(500));
    exact_size(&history);
}

#[test]
fn restore_checks_evicted_states_and_keeps_the_current_document_sixteen_mib_cap() {
    let assets = AssetLibrary::new();
    let initial = document("a");
    let mut invalid = document("old");
    invalid.version = 999;
    let snapshot = HistorySnapshot {
        current: initial.clone(),
        undo: vec![invalid],
        redo: vec![],
        capacity: 8,
    };
    let budget = serde_json::to_vec(&HistorySnapshot {
        current: initial,
        undo: vec![],
        redo: vec![],
        capacity: 8,
    })
    .unwrap()
    .len();
    assert!(ArrangementHistory::from_snapshot_with_budget(snapshot, &assets, budget).is_err());
    let mut oversized = Arrangement::new(8000, 1);
    let name = "x".repeat(500);
    oversized.lanes = (0..29_000)
        .map(|index| Lane::new(format!("lane-{index}"), name.clone()))
        .collect();
    assert!(serde_json::to_vec(&oversized).unwrap().len() > MAX_METADATA_BYTES);
    assert!(ArrangementHistory::new(oversized, &assets, 64).is_err());
}
