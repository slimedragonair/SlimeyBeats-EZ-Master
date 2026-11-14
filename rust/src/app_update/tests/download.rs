use super::*;
use crate::app_update::{UpdateProgress, download::download_with};
use std::{
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

#[test]
fn installer_download_is_bounded_staged_inert_and_can_be_reverified_before_launch() {
    let candidate = available_candidate();
    let root = tempfile::tempdir().unwrap();
    let audio = root.path().join("original.wav");
    std::fs::write(&audio, b"original audio").unwrap();
    let stages = RefCell::new(Vec::new());
    let installer = download_with(
        &candidate,
        root.path(),
        WINDOWS_X86_64,
        cancel(),
        &|stage| stages.borrow_mut().push(stage),
        &|url, hash, destination, max, _, progress| {
            assert!(url.starts_with(
                "https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/"
            ));
            assert_eq!(hash, sha256_hex(b"abc"));
            assert_eq!(max, 3);
            assert!(destination.starts_with(root.path().join("app-updates")));
            assert!(!destination.exists());
            std::fs::write(destination, b"abc")?;
            progress(3);
            Ok(destination.into())
        },
    )
    .unwrap();
    assert_eq!(installer.identity, *candidate.identity());
    assert_eq!(installer.platform, WINDOWS_X86_64);
    assert_eq!(installer.executable_sha256, candidate.executable_sha256());
    assert!(stages.borrow().contains(&UpdateProgress::Downloading {
        received_bytes: 3,
        total_bytes: 3
    }));
    assert_eq!(stages.borrow().last(), Some(&UpdateProgress::Ready));
    installer.verify(cancel()).unwrap();
    std::fs::write(&installer.path, b"xyz").unwrap();
    assert!(installer.verify(cancel()).is_err());
    assert_eq!(std::fs::read(audio).unwrap(), b"original audio");
}

#[test]
fn failed_or_wrong_size_downloads_clean_staging_and_preserve_previous_files() {
    let candidate = available_candidate();
    let root = tempfile::tempdir().unwrap();
    let owned = root.path().join("app-updates");
    std::fs::create_dir(&owned).unwrap();
    let previous = owned.join("previous-installer.exe");
    std::fs::write(&previous, b"keep previous").unwrap();
    for fail in [true, false] {
        let result = download_with(
            &candidate,
            root.path(),
            WINDOWS_X86_64,
            cancel(),
            &|_| {},
            &|_, _, destination, _, _, _| {
                std::fs::write(destination, b"too long")?;
                if fail {
                    bail!("fixture hash mismatch");
                }
                Ok(destination.into())
            },
        );
        assert!(result.is_err());
        assert_eq!(std::fs::read(&previous).unwrap(), b"keep previous");
        assert_eq!(std::fs::read_dir(&owned).unwrap().count(), 1);
    }
}

#[test]
fn cancellation_never_replaces_old_files_and_removes_partial_staging() {
    let candidate = available_candidate();
    let root = tempfile::tempdir().unwrap();
    let token = cancel();
    let result = download_with(
        &candidate,
        root.path(),
        WINDOWS_X86_64,
        token.clone(),
        &|_| {},
        &|_, _, destination, _, cancellation, _| {
            assert!(Arc::ptr_eq(&token, &cancellation));
            std::fs::write(destination, b"abc")?;
            token.store(true, Ordering::Relaxed);
            Ok(destination.into())
        },
    );
    assert!(result.is_err());
    assert_eq!(
        std::fs::read_dir(root.path().join("app-updates"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn unsupported_platform_relative_cache_and_precancelled_requests_never_fetch() {
    let candidate = available_candidate();
    let root = tempfile::tempdir().unwrap();
    let calls = AtomicUsize::new(0);
    for (platform, path, cancellation) in [
        ("linux_x86_64", root.path(), cancel()),
        (WINDOWS_X86_64, Path::new("relative"), cancel()),
        (WINDOWS_X86_64, root.path(), Arc::new(AtomicBool::new(true))),
    ] {
        let result = download_with(
            &candidate,
            path,
            platform,
            cancellation,
            &|_| {},
            &|_, _, _, _, _, _| {
                calls.fetch_add(1, Ordering::Relaxed);
                bail!("Must not fetch")
            },
        );
        assert!(result.is_err());
    }
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert!(!root.path().join("app-updates").exists());
}

#[test]
fn downloader_cannot_return_an_unrelated_path() {
    let candidate = available_candidate();
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original.wav");
    std::fs::write(&original, b"abc").unwrap();
    assert!(
        download_with(
            &candidate,
            root.path(),
            WINDOWS_X86_64,
            cancel(),
            &|_| {},
            &|_, _, _, _, _, _| Ok(original.clone())
        )
        .is_err()
    );
    assert_eq!(std::fs::read(original).unwrap(), b"abc");
}

#[test]
fn cancelled_launch_reverification_is_an_error() {
    let candidate = available_candidate();
    let root = tempfile::tempdir().unwrap();
    let installer = download_with(
        &candidate,
        root.path(),
        WINDOWS_X86_64,
        cancel(),
        &|_| {},
        &|_, _, path, _, _, _| {
            std::fs::write(path, b"abc")?;
            Ok(path.into())
        },
    )
    .unwrap();
    assert!(installer.verify(Arc::new(AtomicBool::new(true))).is_err());
}

#[cfg(unix)]
#[test]
fn redirected_owned_cache_is_rejected_without_touching_the_target() {
    let candidate = available_candidate();
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("app-updates")).unwrap();
    assert!(
        download_with(
            &candidate,
            root.path(),
            WINDOWS_X86_64,
            cancel(),
            &|_| {},
            &|_, _, _, _, _, _| panic!("Redirected cache must never fetch")
        )
        .is_err()
    );
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}
