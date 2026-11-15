use super::*;
use std::{collections::BTreeSet, sync::Arc};

#[test]
fn repeated_unused_caches_and_connection_ids_do_not_allocate_scratch() {
    let root = tempfile::tempdir().unwrap();
    let mut ids = BTreeSet::new();
    for _ in 0..512 {
        let cache = SessionCache::new(root.path().into());
        assert!(ids.insert(connection_id()), "Connection IDs must be fresh");
        assert!(cache.directory.get().is_none());
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn concurrent_scratch_requests_share_one_directory_on_the_output_volume() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().canonicalize().unwrap();
    let cache = Arc::new(SessionCache::new(output.clone()));
    let threads = (0..8)
        .map(|_| {
            let cache = cache.clone();
            std::thread::spawn(move || cache.path().unwrap().to_path_buf())
        })
        .collect::<Vec<_>>();
    let paths = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(paths.len(), 1);
    let path = paths.first().unwrap();
    assert_eq!(path.parent(), Some(output.as_path()));
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 1);
    drop(cache);
    assert!(!path.exists());
}

#[test]
fn failed_creation_can_be_retried_without_reconnecting() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("temporarily unavailable output");
    let cache = SessionCache::new(output.clone());
    assert!(cache.path().is_err());
    assert!(cache.directory.get().is_none());
    std::fs::create_dir(&output).unwrap();
    assert!(cache.path().unwrap().is_dir());
}

#[test]
fn drop_only_removes_its_own_scratch_and_preserves_other_connections_and_projects() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("User project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("source.wav"), b"saved project").unwrap();
    let unrelated = root.path().join(".slimeybeats-mcp-user-owned");
    std::fs::create_dir(&unrelated).unwrap();
    let other = SessionCache::new(root.path().into());
    let other_path = other.path().unwrap().to_path_buf();
    std::fs::write(other_path.join("preview.wav"), b"other live connection").unwrap();
    let owned = SessionCache::new(root.path().into());
    let owned_path = owned.path().unwrap().to_path_buf();
    std::fs::write(owned_path.join("preview.wav"), b"temporary audio").unwrap();
    drop(owned);
    assert!(!owned_path.exists());
    assert_eq!(
        std::fs::read(project.join("source.wav")).unwrap(),
        b"saved project"
    );
    assert!(unrelated.is_dir());
    assert_eq!(
        std::fs::read(other_path.join("preview.wav")).unwrap(),
        b"other live connection"
    );
}
