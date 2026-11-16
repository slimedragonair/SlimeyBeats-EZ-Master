use super::{Topic, get};

#[test]
fn hybrid_help_explains_one_editor_workflow_and_local_mcp_boundary() {
    let help = get(Topic::Hybrid);
    let workflow = help["browser_workflow"].as_str().unwrap();
    for expected in [
        "checks availability without uploading audio",
        "same Add audio, Analyze, Master, live EQ and Export",
        "Only the normal Master button sends",
        "same A/B player",
        "Switching off keeps the received master editable",
        "does not confirm deletion",
        "No checkbox or second upload form",
    ] {
        assert!(workflow.contains(expected), "Missing {expected}");
    }
    assert!(
        help["local_companion"]
            .as_str()
            .unwrap()
            .contains("do not connect to the hybrid HTTP service")
    );
}

#[test]
fn hybrid_help_explains_expired_access_without_replaying_audio() {
    let help = get(Topic::Hybrid);
    let recovery = help["unavailable_job"].as_str().unwrap();
    for expected in [
        "401 or 404",
        "Retry",
        "requests no server deletion and confirms none",
        "previously reported expiry",
        "Local audio and settings stay unchanged",
    ] {
        assert!(recovery.contains(expected), "Missing {expected}");
    }
    let reconnect = help["reconnect"].as_str().unwrap();
    for expected in [
        "never replays an upload or mastering job",
        "does not discard local audio or settings",
        "Press Master",
        "Recovery restores saved server PCM locally",
        "duration/memory limits still apply",
    ] {
        assert!(reconnect.contains(expected), "Missing {expected}");
    }
}
