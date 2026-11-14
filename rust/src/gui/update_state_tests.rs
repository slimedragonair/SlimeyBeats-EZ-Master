use super::*;

fn installer(path: PathBuf) -> VerifiedInstaller {
    VerifiedInstaller {
        path,
        identity: app_update::current(),
        platform: app_update::WINDOWS_X86_64.into(),
        installer_sha256: "a".repeat(64),
        installer_bytes: 3,
        executable_sha256: "b".repeat(64),
        release_url: app_update::MANUAL_RELEASE_URL.into(),
    }
}

fn downloaded(candidate: &UpdateCandidate) -> VerifiedInstaller {
    VerifiedInstaller {
        path: "ready.exe".into(),
        identity: candidate.identity().clone(),
        platform: candidate.platform().into(),
        installer_sha256: candidate.installer_sha256().into(),
        installer_bytes: candidate.installer_bytes(),
        executable_sha256: candidate.executable_sha256().into(),
        release_url: candidate.release_url().into(),
    }
}
#[test]
fn a_newer_catalog_cannot_display_new_notes_but_install_an_older_download() {
    let old =
        app_update::fixture_candidate(app_update::identity::BUILD_REVISION + 1, &"a".repeat(64));
    let new =
        app_update::fixture_candidate(app_update::identity::BUILD_REVISION + 2, &"b".repeat(64));
    let mut state = UpdateState::for_tests();
    state.downloaded = Some(downloaded(&old));
    let sender = state.begin();
    sender
        .send(Event::Checked(Ok(CheckResult::Available(new))))
        .unwrap();
    state.poll();
    assert!(state.downloaded.is_none());
    assert_eq!(
        state.available.as_ref().unwrap().build_revision(),
        old.build_revision() + 1
    );
}
#[test]
fn rechecking_the_exact_catalog_retains_its_verified_download() {
    let candidate =
        app_update::fixture_candidate(app_update::identity::BUILD_REVISION + 1, &"a".repeat(64));
    let mut state = UpdateState::for_tests();
    state.downloaded = Some(downloaded(&candidate));
    let sender = state.begin();
    sender
        .send(Event::Checked(Ok(CheckResult::Available(candidate))))
        .unwrap();
    state.poll();
    assert_eq!(
        state.downloaded.as_ref().unwrap().path,
        PathBuf::from("ready.exe")
    );
}

#[test]
fn update_preferences_survive_restart_without_changing_audio_preferences() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("updates.json");
    let audio = root.path().join("rust-settings.json");
    std::fs::write(&audio, b"user audio settings").unwrap();
    let mut state = UpdateState::load(path.clone());
    state.preferences.automatic = false;
    state.preferences.last_check = 123;
    state.persist(false);
    let restored = UpdateState::load(path);
    assert!(!restored.preferences.automatic);
    assert_eq!(restored.preferences.last_check, 123);
    assert_eq!(std::fs::read(audio).unwrap(), b"user audio settings");
}
#[test]
fn corrupt_and_oversized_preferences_stay_untouched_until_explicit_save() {
    for data in [
        b"broken json".to_vec(),
        b"{\"unrelated_user_data\":true}".to_vec(),
        vec![b'x'; 17000],
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("updates.json");
        std::fs::write(&path, &data).unwrap();
        let mut state = UpdateState::load(path.clone());
        assert!(state.save_error.is_some());
        state.preferences.automatic = true;
        state.persist(false);
        assert_eq!(std::fs::read(&path).unwrap(), data);
        state.persist(true);
        assert!(state.save_error.is_none());
        assert!(UpdateState::load(path).preferences.automatic);
    }
}
#[test]
fn cancelled_download_never_becomes_ready_or_closes_the_application() {
    let mut state = UpdateState::for_tests();
    let sender = state.begin();
    state.cancel();
    sender
        .send(Event::Downloaded(Ok(installer("not-executed.exe".into()))))
        .unwrap();
    state.poll();
    assert!(!state.busy());
    assert!(state.downloaded.is_none());
    assert!(!state.close_after_launch);
    assert!(state.message.contains("Cancelled"));
}
#[test]
fn worker_disconnect_recovers_and_failed_launch_leaves_the_application_open() {
    let mut state = UpdateState::for_tests();
    let sender = state.begin();
    state.opening_setup = true;
    drop(sender);
    state.poll();
    assert!(!state.busy() && !state.opening_setup);
    assert!(state.error.is_some());
    let sender = state.begin();
    sender
        .send(Event::Launched(Err("fixture failed".into())))
        .unwrap();
    state.poll();
    assert!(!state.close_after_launch);
    assert_eq!(state.error.as_deref(), Some("fixture failed"));
}
#[test]
fn opening_setup_is_an_explicit_committed_action_not_a_cancelled_download() {
    let mut state = UpdateState::for_tests();
    let sender = state.begin();
    state.opening_setup = true;
    state.cancel();
    assert!(!state.cancel.load(Ordering::Relaxed));
    sender.send(Event::Launched(Ok(()))).unwrap();
    state.poll();
    assert!(state.close_after_launch);
}
#[test]
fn startup_fixture_and_disabled_preference_never_spawn_network_work() {
    let ctx = egui::Context::default();
    let mut state = UpdateState::for_tests();
    state.startup(&ctx, true);
    assert!(!state.busy());
    state.startup_checked = false;
    state.startup(&ctx, false);
    assert!(!state.startup_checked);
    state.startup(&ctx, true);
    assert!(state.startup_checked);
    assert!(!state.busy());
}

#[test]
fn modified_verified_download_is_refused_before_installer_execution() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("setup.exe");
    std::fs::write(&path, b"bad").unwrap();
    assert!(
        super::super::update_launch::open(
            &installer(path.clone()),
            Arc::new(AtomicBool::new(false))
        )
        .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), b"bad");
}
