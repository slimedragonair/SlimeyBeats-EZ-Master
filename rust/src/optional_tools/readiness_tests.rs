use super::{Runtime, readiness};
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

fn pair(root: &std::path::Path) -> Runtime {
    let runtime = Runtime {
        ffmpeg: root.join("ffmpeg"),
        ffprobe: root.join("ffprobe"),
        private: false,
    };
    fs::write(&runtime.ffmpeg, b"has a valid version banner").unwrap();
    fs::write(&runtime.ffprobe, b"has a valid version banner").unwrap();
    runtime
}

#[test]
fn version_banners_cannot_approve_missing_encoders_and_failure_is_cached() {
    let root = tempfile::tempdir().unwrap();
    let runtime = pair(root.path());
    let calls = AtomicUsize::new(0);
    let smoke = |_: &Runtime, _: &std::path::Path, _: &Arc<AtomicBool>| -> anyhow::Result<()> {
        calls.fetch_add(1, Ordering::Relaxed);
        anyhow::bail!("libmp3lame encoder unavailable");
    };
    for _ in 0..2 {
        assert!(
            readiness::check(
                &runtime,
                root.path(),
                &Arc::new(AtomicBool::new(false)),
                &smoke
            )
            .is_err()
        );
    }
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(readiness::state(&runtime), Some(false));
    assert!(super::probes::prepared_candidate(runtime, None, None).is_none());
}

#[test]
fn lookup_accepts_only_a_functionally_prepared_pair_from_the_current_path() {
    let root = tempfile::tempdir().unwrap();
    let runtime = pair(root.path());
    assert!(super::probes::prepared_candidate(runtime.clone(), None, None).is_none());
    readiness::remember(&runtime, true).unwrap();
    assert_eq!(
        super::probes::prepared_candidate(runtime.clone(), None, None),
        Some(runtime.clone())
    );
    assert!(
        super::probes::prepared_candidate(
            runtime,
            Some(std::ffi::OsStr::new("old path")),
            Some(std::ffi::OsStr::new("new path"))
        )
        .is_none()
    );
}

#[test]
fn cached_pair_readiness_expires_when_either_executable_changes() {
    let root = tempfile::tempdir().unwrap();
    let runtime = pair(root.path());
    readiness::remember(&runtime, true).unwrap();
    assert_eq!(readiness::state(&runtime), Some(true));
    fs::write(&runtime.ffprobe, b"changed probe").unwrap();
    assert_eq!(readiness::state(&runtime), None);
    readiness::remember(&runtime, true).unwrap();
    fs::write(&runtime.ffmpeg, b"changed encoder").unwrap();
    assert_eq!(readiness::state(&runtime), None);
}

#[test]
fn cancellation_never_records_a_failed_or_ready_pair() {
    let root = tempfile::tempdir().unwrap();
    let runtime = pair(root.path());
    let cancel = Arc::new(AtomicBool::new(false));
    assert!(
        readiness::check(&runtime, root.path(), &cancel, &|_, _, _| {
            cancel.store(true, Ordering::Relaxed);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(readiness::state(&runtime), None);
}

#[test]
fn replacing_an_executable_during_probes_cannot_approve_its_replacement() {
    let root = tempfile::tempdir().unwrap();
    let runtime = pair(root.path());
    assert!(
        readiness::check(
            &runtime,
            root.path(),
            &Arc::new(AtomicBool::new(false)),
            &|_, _, _| {
                fs::write(&runtime.ffmpeg, b"different executable")?;
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(readiness::state(&runtime), None);
}
