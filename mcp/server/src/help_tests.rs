use super::{Topic, get};

#[test]
fn hybrid_help_explains_one_editor_workflow_and_local_mcp_boundary() {
    let help = get(Topic::Hybrid);
    let workflow = help["browser_workflow"].as_str().unwrap();
    for expected in [
        "checks availability without uploading audio",
        "same Add audio, Analyze, Master, live EQ and Export",
        "importing uploads the original file",
        "supported heavy Studio tools run on the server",
        "Reference imports upload reference audio",
        "streamed previews",
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
        "Switching off does not turn streamed audio into full local PCM",
        "server-backed heavy edits/exports require server access",
        "Recovering a saved local PCM copy does not reconnect or upload",
    ] {
        assert!(reconnect.contains(expected), "Missing {expected}");
    }
}

#[test]
fn hybrid_help_separates_source_status_and_local_mcp_transport() {
    let help = get(Topic::Hybrid);
    assert_eq!(help["source_status"]["phase"], "source_candidate");
    assert_eq!(help["source_status"]["browser_api_version"], 2);
    assert_eq!(help["source_status"]["deployment_verified"], false);
    assert_eq!(help["remote_processing_from_mcp"], false);
    assert_eq!(help["public_http_transport"], false);
    assert!(
        help["agent_action"]
            .as_str()
            .unwrap()
            .contains("exports locally")
    );
    assert!(help["setup"].as_str().unwrap().contains("Never expose"));
}

#[test]
fn hybrid_help_distinguishes_large_server_admission_from_neural_and_local_caps() {
    let help = get(Topic::Hybrid);
    let server = &help["limits"]["server"];
    assert_eq!(server["max_audio_seconds"], 3600);
    assert_eq!(server["max_upload_bytes"], 2_147_483_648_u64);
    assert_eq!(server["max_decoded_bytes"], 2_147_483_648_u64);
    assert_eq!(server["separation_max_decoded_bytes"], 536_870_912_u64);
    assert_eq!(server["retention_seconds"], 3600);
    assert_eq!(server["preview_kbps"], 192);
    assert_eq!(help["limits"]["local_browser"]["max_audio_seconds"], 900);
    assert_eq!(
        help["limits"]["local_browser"]["studio_max_origin_decoded_bytes"],
        134_217_728_u64
    );
    assert!(
        help["limits"]["memory_boundary"]
            .as_str()
            .unwrap()
            .contains("not guaranteed working-memory capacity")
    );
}

#[test]
fn hybrid_help_keeps_preview_exports_and_remote_recovery_boundaries_distinct() {
    let help = get(Topic::Hybrid);
    let preview = help["preview"].as_str().unwrap();
    assert!(preview.contains("lossy preview, not the export source"));
    assert!(preview.contains("full-resolution audio"));
    let recovery = help["recovery"].as_str().unwrap();
    for expected in [
        "at most one hour",
        "session ZIP",
        "does not download or save full streamed server projects",
        "does not confirm deletion",
    ] {
        assert!(recovery.contains(expected), "Missing {expected}");
    }
    assert_eq!(
        help["documentation"]["protocol"],
        "docs/HYBRID_REMOTE_PROTOCOL.md"
    );
    assert_eq!(
        help["documentation"]["benchmark"],
        "hybrid/deploy/remote-tools/BENCHMARK.md"
    );
}
