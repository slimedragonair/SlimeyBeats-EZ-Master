use super::*;

#[test]
fn an_older_window_cannot_restore_the_tutorial_after_another_window_finishes_it() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("interface-preferences.json");
    let mut older = UserExperience::load(path.clone());
    let mut newer = UserExperience::load(path.clone());
    newer.finish_tour(true);
    older.preferences.advanced = true;
    older.persist(true, false);
    let mut reopened = UserExperience::load(path);
    reopened.offer_when_idle(true);
    assert!(
        !reopened.welcome_open,
        "Changing controls in an older window must not reset a saved tutorial choice"
    );
}

#[test]
fn a_stale_write_or_a_removed_ui_snapshot_cannot_erase_the_saved_choice() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("interface-preferences.json");
    let mut first = UserExperience::load(path.clone());
    first.finish_tour(false);
    // Simulate a stale writer that read before the one-way marker was created.
    std::fs::write(&path, "{}").unwrap();
    let mut stale = UserExperience::load(path.clone());
    stale.offer_when_idle(true);
    assert!(!stale.welcome_open);
    std::fs::remove_file(&path).unwrap();
    let mut missing = UserExperience::load(path);
    missing.offer_when_idle(true);
    assert!(!missing.welcome_open);
    missing.replay_tour();
    assert!(missing.tour_open, "Explicit replay remains available");
}

#[test]
fn completed_declined_and_closed_tours_stay_suppressed_after_restarts() {
    let folder = tempfile::tempdir().unwrap();
    for completed in [true, false] {
        let path = folder
            .path()
            .join(format!("profile-{completed}"))
            .join("interface-preferences.json");
        let mut first = UserExperience::load(path.clone());
        first.offer_when_idle(false);
        assert!(!first.welcome_open);
        first.offer_when_idle(true);
        assert!(first.welcome_open);
        if completed {
            first.start_tour();
        }
        first.finish_tour(completed);
        assert!(
            path.is_file(),
            "The choice must save immediately, before a debounce or exit"
        );
        let mut reopened = UserExperience::load(path);
        reopened.offer_when_idle(true);
        assert!(!reopened.welcome_open && !reopened.tour_open);
        let status = reopened.preferences.tutorial_status;
        reopened.replay_tour();
        assert!(reopened.tour_open);
        assert_eq!(
            reopened.preferences.tutorial_status, status,
            "Replay must not clear saved suppression"
        );
        reopened.persist(true, false);
    }
}

#[test]
fn an_interrupted_tour_can_resume_but_diagnostic_captures_do_not_offer_it() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("preferences.json");
    let mut first = UserExperience::load(path.clone());
    first.start_tour();
    first.preferences.tutorial_step = 3;
    first.persist(true, false);
    let mut reopened = UserExperience::load(path.clone());
    assert_eq!(reopened.preferences.tutorial_step, 3);
    reopened.offer_when_idle(true);
    assert!(reopened.welcome_open);
    let mut diagnostic = UserExperience::load(path);
    diagnostic.suppress_automatic_offer();
    diagnostic.offer_when_idle(true);
    assert!(!diagnostic.welcome_open);
}

#[test]
fn unreadable_interface_preferences_do_not_interrupt_audio_or_repeat_prompts() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("interface-preferences.json");
    std::fs::write(&path, "damaged preferences").unwrap();
    let mut first = UserExperience::load(path.clone());
    first.offer_when_idle(true);
    assert!(!first.welcome_open);
    assert!(first.save_error.is_some());
    first.preferences.advanced = true;
    first.persist(true, false);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "damaged preferences"
    );
    first.persist(true, true);
    assert!(first.save_error.is_none());
    let reopened = UserExperience::load(path);
    assert!(reopened.advanced());
    assert!(!reopened.preferences.should_offer());
}
