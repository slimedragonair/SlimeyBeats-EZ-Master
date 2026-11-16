use super::{Topic, get};

#[test]
fn hybrid_help_distinguishes_disconnect_from_page_close() {
    let help = get(Topic::Hybrid);
    let workflow = help["browser_workflow"].as_str().unwrap();
    for expected in [
        "checks availability without uploading audio",
        "Switching off preserves existing job access in the tab",
        "Disconnect clears the credential but retains the job reference",
        "closing the page forgets the entire job reference",
        "Neither action confirms server deletion",
    ] {
        assert!(workflow.contains(expected), "Missing {expected}");
    }
    assert!(!workflow.contains("Disconnect or closing the page forgets"));
    assert!(
        help["local_companion"]
            .as_str()
            .unwrap()
            .contains("do not connect to the hybrid HTTP service")
    );
}

#[test]
fn hybrid_help_explains_local_unavailable_job_recovery_and_explicit_reconnection() {
    let help = get(Topic::Hybrid);
    let recovery = help["unavailable_job"].as_str().unwrap();
    for expected in [
        "stale or expired job returning 401 or 404",
        "Forget unavailable job",
        "job reference, server credential and upload consent in this tab only",
        "requests no server deletion and confirms none",
        "previously reported expiry",
        "Local audio and settings stay unchanged",
    ] {
        assert!(recovery.contains(expected), "Missing {expected}");
    }
    let reconnect = help["reconnect"].as_str().unwrap();
    for expected in [
        "Check & connect as a separate action",
        "new guest session",
        "never replays an upload or mastering job",
        "does not discard local audio or settings",
        "fresh explicit upload consent",
    ] {
        assert!(reconnect.contains(expected), "Missing {expected}");
    }
}
