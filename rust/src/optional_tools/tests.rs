use super::{
    archive_tests::{hash, zip_bytes},
    pins::Pin,
    *,
};
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

struct Fixture {
    temp: tempfile::TempDir,
    archive: PathBuf,
    archive_hash: String,
    payload_hash: String,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("fixture.zip");
        let members = pins::WINDOWS.members();
        let bytes = zip_bytes(
            &members
                .iter()
                .map(|m| (m.as_str(), b"fixture".as_slice()))
                .collect::<Vec<_>>(),
        );
        fs::write(&archive, &bytes).unwrap();
        Self {
            temp,
            archive,
            archive_hash: hash(&bytes),
            payload_hash: hash(b"fixture"),
        }
    }
    fn paths(&self) -> Paths {
        Paths::with_root(self.temp.path().join("private cache"))
    }
    fn pin(&self) -> Pin<'_> {
        Pin {
            sha256: &self.archive_hash,
            hashes: [&self.payload_hash; 3],
            ..pins::WINDOWS
        }
    }
    fn fetch(
        &self,
        _: &str,
        _: &str,
        to: &Path,
        _: usize,
        cancel: Arc<AtomicBool>,
        progress: &dyn Fn(u64),
    ) -> anyhow::Result<PathBuf> {
        archive::check_cancel(&cancel)?;
        fs::copy(&self.archive, to)?;
        progress(fs::metadata(to)?.len());
        Ok(to.into())
    }
    fn install(&self, paths: &Paths) -> anyhow::Result<Runtime> {
        let runtime = paths.ensure_with(
            &self.pin(),
            &|_, _| {},
            Arc::new(AtomicBool::new(false)),
            |a, b, c, d, e, f| self.fetch(a, b, c, d, e, f),
            |_, _, _| Ok(()),
        )?;
        // These intentionally nonexecutable fixtures require repair on subsequent calls.
        readiness::remember(&runtime, false)?;
        Ok(runtime)
    }
}

#[test]
fn publishes_only_verified_fixed_files_and_receipt_last() {
    let f = Fixture::new();
    let p = f.paths();
    let calls = AtomicUsize::new(0);
    let result = p
        .ensure_with(
            &f.pin(),
            &|_, _| {},
            Arc::new(AtomicBool::new(false)),
            |a, b, c, d, e, f2| f.fetch(a, b, c, d, e, f2),
            |runtime, _, _| {
                assert!(runtime.ffmpeg.is_absolute());
                assert!(!p.root.join("ready.json").exists());
                assert_eq!(fs::read(&runtime.ffprobe)?, b"fixture");
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(result.private && result.ffmpeg.is_file() && result.ffprobe.is_file());
    let payload = result.ffmpeg.parent().unwrap();
    assert_eq!(fs::read_dir(payload).unwrap().count(), 5);
    let notice = fs::read_to_string(payload.join("NOTICE.txt")).unwrap();
    assert!(notice.contains(pins::FFMPEG_SOURCE) && notice.contains(&f.archive_hash));
    assert_eq!(
        fs::read_dir(&p.root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e
                .file_name()
                .to_string_lossy()
                .starts_with(".ffmpeg-stage-"))
            .count(),
        0
    );
}
#[test]
fn bad_archive_hash_or_binary_hash_never_reaches_smoke_or_publish() {
    for bad_archive in [true, false] {
        let f = Fixture::new();
        let p = f.paths();
        let bad = "0".repeat(64);
        let mut pin = f.pin();
        if bad_archive {
            pin.sha256 = &bad;
        } else {
            pin.hashes[0] = &bad;
        }
        assert!(
            p.ensure_with(
                &pin,
                &|_, _| {},
                Arc::new(AtomicBool::new(false)),
                |a, b, c, d, e, f2| f.fetch(a, b, c, d, e, f2),
                |_, _, _| panic!("unverified executable was probed")
            )
            .is_err()
        );
        assert!(!p.root.join("ready.json").exists());
    }
}
#[test]
fn failed_repair_preserves_previous_receipt_and_payload() {
    let f = Fixture::new();
    let p = f.paths();
    let previous = f.install(&p).unwrap();
    let receipt = fs::read(p.root.join("ready.json")).unwrap();
    let error = p.ensure_with(
        &f.pin(),
        &|_, _| {},
        Arc::new(AtomicBool::new(false)),
        |a, b, c, d, e, f2| f.fetch(a, b, c, d, e, f2),
        |_, _, _| anyhow::bail!("smoke failure"),
    );
    assert!(error.is_err());
    assert_eq!(fs::read(p.root.join("ready.json")).unwrap(), receipt);
    assert_eq!(fs::read(previous.ffmpeg).unwrap(), b"fixture");
    assert_eq!(fs::read_dir(p.root.join("installs")).unwrap().count(), 1);
}

#[test]
fn successful_replacement_keeps_previous_install_and_switches_receipt() {
    let f = Fixture::new();
    let p = f.paths();
    let previous = f.install(&p).unwrap();
    let receipt = fs::read(p.root.join("ready.json")).unwrap();
    let replacement = f.install(&p).unwrap();
    assert_ne!(replacement.ffmpeg, previous.ffmpeg);
    assert_ne!(fs::read(p.root.join("ready.json")).unwrap(), receipt);
    assert_eq!(fs::read(previous.ffmpeg).unwrap(), b"fixture");
    assert_eq!(fs::read(replacement.ffmpeg).unwrap(), b"fixture");
    assert_eq!(fs::read_dir(p.root.join("installs")).unwrap().count(), 2);
}

#[test]
fn cancelled_receipt_publication_cannot_replace_previous_pointer() {
    let f = Fixture::new();
    let p = f.paths();
    f.install(&p).unwrap();
    let previous = fs::read(p.root.join("ready.json")).unwrap();
    let next = cache::Receipt::new(&f.pin(), "runtime-unpublished".into());
    assert!(cache::publish_receipt(&p, &next, &AtomicBool::new(true)).is_err());
    assert_eq!(fs::read(p.root.join("ready.json")).unwrap(), previous);
}

#[test]
fn cached_integrity_check_detects_same_size_modified_payload() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("tool");
    fs::write(&file, b"original").unwrap();
    probes::verify(&file, &hash(b"original"), &AtomicBool::new(false)).unwrap();
    std::thread::sleep(Duration::from_millis(10));
    fs::write(&file, b"modified").unwrap();
    assert!(probes::verify(&file, &hash(b"original"), &AtomicBool::new(false)).is_err());
}
#[test]
fn cancellations_during_download_extraction_and_probe_preserve_previous_install() {
    for phase in [0.02, 0.65, 0.85] {
        let f = Fixture::new();
        let p = f.paths();
        let previous = f.install(&p).unwrap();
        let receipt = fs::read(p.root.join("ready.json")).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let result = p.ensure_with(
            &f.pin(),
            &|fraction, _| {
                if fraction == phase {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
            cancel.clone(),
            |a, b, c, d, e, f2| f.fetch(a, b, c, d, e, f2),
            |_, _, _| Ok(()),
        );
        assert!(result.is_err(), "{phase}");
        assert_eq!(fs::read(p.root.join("ready.json")).unwrap(), receipt);
        assert!(previous.ffprobe.exists());
        assert_eq!(fs::read_dir(p.root.join("installs")).unwrap().count(), 1);
    }
}
#[test]
fn pre_cancelled_setup_is_read_only_and_unowned_or_relative_roots_are_rejected() {
    let f = Fixture::new();
    let p = f.paths();
    assert!(
        p.ensure_with(
            &f.pin(),
            &|_, _| {},
            Arc::new(AtomicBool::new(true)),
            |_, _, _, _, _, _| panic!(),
            |_, _, _| panic!()
        )
        .is_err()
    );
    assert!(!p.root.exists());
    fs::create_dir(&p.root).unwrap();
    fs::write(p.root.join("unrelated"), b"keep").unwrap();
    assert!(f.install(&p).is_err());
    assert_eq!(fs::read_dir(p.root()).unwrap().count(), 1);
    assert!(f.install(&Paths::with_root("relative-cache")).is_err());
}
#[test]
fn lock_is_exclusive_cancellable_and_released_after_drop() {
    let root = tempfile::tempdir().unwrap();
    let p = Paths::with_root(root.path().join("cache"));
    cache::prepare(&p).unwrap();
    let held = cache::lock(&p, &AtomicBool::new(false), &|| {}).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let (notify, waiting) = std::sync::mpsc::sync_channel(1);
    let worker = {
        let p = p.clone();
        let cancel = cancel.clone();
        std::thread::spawn(move || cache::lock(&p, &cancel, &|| notify.send(()).unwrap()).is_err())
    };
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    cancel.store(true, Ordering::Relaxed);
    assert!(worker.join().unwrap());
    drop(held);
    assert!(cache::lock(&p, &AtomicBool::new(false), &|| {}).is_ok());
}

#[test]
fn a_waiting_setup_acquires_the_lock_after_the_other_window_finishes() {
    let root = tempfile::tempdir().unwrap();
    let p = Paths::with_root(root.path().join("cache"));
    cache::prepare(&p).unwrap();
    let held = cache::lock(&p, &AtomicBool::new(false), &|| {
        panic!("uncontended setup reported waiting")
    })
    .unwrap();
    let (notify, waiting) = std::sync::mpsc::sync_channel(1);
    let worker = {
        let p = p.clone();
        std::thread::spawn(move || {
            cache::lock(&p, &AtomicBool::new(false), &|| notify.send(()).unwrap())
        })
    };
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    drop(held);
    let handed_off = worker.join().unwrap().unwrap();
    drop(handed_off);
    assert!(cache::lock(&p, &AtomicBool::new(false), &|| {}).is_ok());
}
#[test]
fn receipt_cannot_redirect_lookup_outside_the_owned_root() {
    let f = Fixture::new();
    let p = f.paths();
    f.install(&p).unwrap();
    let mut receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(p.root.join("ready.json")).unwrap()).unwrap();
    receipt["installation"] = "../../elsewhere".into();
    fs::write(
        p.root.join("ready.json"),
        serde_json::to_vec(&receipt).unwrap(),
    )
    .unwrap();
    assert!(cache::ready(&p, &f.pin(), &Arc::new(AtomicBool::new(false))).is_err());
}

#[test]
fn pinned_receipt_lookup_does_not_require_in_process_capability_state() {
    let fixture = Fixture::new();
    let paths = fixture.paths();
    let runtime = fixture.install(&paths).unwrap();
    assert_ne!(readiness::state(&runtime), Some(true));
    assert_eq!(
        cache::ready(&paths, &fixture.pin(), &Arc::new(AtomicBool::new(false))).unwrap(),
        Some(runtime.clone())
    );
    fs::write(&runtime.ffprobe, b"corrupted probe contents").unwrap();
    assert!(cache::ready(&paths, &fixture.pin(), &Arc::new(AtomicBool::new(false))).is_err());
}
#[test]
fn lookup_never_downloads_or_creates_cache_and_rejects_arbitrary_tools() {
    let f = Fixture::new();
    let p = f.paths();
    assert!(p.locate("vst-plugin").is_none());
    assert!(p.locate("../ffmpeg").is_none());
    let _ = p.locate("ffmpeg");
    let _ = p.available();
    assert!(!p.root.exists());
}
#[test]
fn absolute_system_path_is_preferred_and_relative_path_entries_are_ignored() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("system bin");
    fs::create_dir(&bin).unwrap();
    let file = bin.join(if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    });
    fs::write(&file, b"fixture").unwrap();
    let path = std::env::join_paths([PathBuf::from("."), PathBuf::from("relative"), bin]).unwrap();
    assert_eq!(
        probes::system_in(Some(&path), "ffmpeg", &|_| true),
        Some(file.canonicalize().unwrap())
    );
    assert!(probes::system_in(Some(&path), "ffmpeg", &|_| false).is_none());
}

#[test]
#[ignore = "requires explicitly supplied downloaded pinned native bundle; does not use network"]
fn live_native_bundle_smoke_and_cached_reuse() {
    let archive = PathBuf::from(
        std::env::var_os("SLIMEYBEATS_FFMPEG_TEST_ARCHIVE").expect("test archive path"),
    );
    let root = tempfile::tempdir().unwrap();
    let paths = Paths::with_root(root.path().join("private runtime"));
    let pin = pins::current().expect("a pinned native target");
    let result = paths
        .ensure_with(
            pin,
            &|_, _| {},
            Arc::new(AtomicBool::new(false)),
            |_, _, dest, _, _, _| {
                fs::copy(&archive, dest)?;
                Ok(dest.into())
            },
            smoke::verify,
        )
        .unwrap();
    assert!(result.ffmpeg.is_absolute());
    assert_eq!(paths.locate("ffmpeg"), Some(result.ffmpeg.clone()));
    assert_eq!(paths.locate("ffprobe"), Some(result.ffprobe.clone()));
    assert!(
        cache::ready(&paths, pin, &Arc::new(AtomicBool::new(false)))
            .unwrap()
            .is_some()
    );
    let reused = paths
        .ensure_with(
            pin,
            &|_, _| {},
            Arc::new(AtomicBool::new(false)),
            |_, _, _, _, _, _| panic!("ready runtime redownloaded"),
            |_, _, _| panic!("ready runtime resmoked"),
        )
        .unwrap();
    assert_eq!(result, reused);
}

#[test]
#[ignore = "requires raw tar from explicitly downloaded and verified pinned Linux bundle"]
fn live_linux_tar_extracts_only_pinned_files() {
    let archive =
        PathBuf::from(std::env::var_os("SLIMEYBEATS_FFMPEG_TEST_TAR").expect("test tar path"));
    let root = tempfile::tempdir().unwrap();
    let members = pins::LINUX.members();
    let entries: Vec<_> = members
        .iter()
        .zip(pins::LINUX.files)
        .map(|(path, file_name)| archive::FixedEntry {
            archive_path: path,
            file_name,
        })
        .collect();
    tar::extract(
        &mut fs::File::open(archive).unwrap(),
        root.path(),
        &entries,
        archive::FFMPEG_LIMITS,
        &AtomicBool::new(false),
    )
    .unwrap();
    cache::verify_files(root.path(), &pins::LINUX, &AtomicBool::new(false)).unwrap();
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 3);
}
