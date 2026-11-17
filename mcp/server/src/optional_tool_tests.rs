//! Consent and job boundaries use fake preparation closures, never real downloads.
use super::*;
use crate::{
    arguments::Arguments,
    optional_tool_protocol::{PrepareToolRequest, ToolStatusRequest},
};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn engine(root: &std::path::Path) -> Arc<Engine> {
    Engine::new(&Arguments {
        input_root: vec![root.into()],
        output_root: root.join("exports"),
        max_decoded_mib: 8,
        decode_timeout_seconds: 5,
        desktop_exe: None,
        profiles_root: None,
        studio_data: None,
        plugin_root: vec![],
        tool_profile: crate::tool_catalog::ToolProfile::Compact,
        print_config: false,
    })
    .unwrap()
}

fn terminal(engine: &Engine, id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let job = engine.job(id).unwrap();
        if ["completed", "failed", "cancelled"].contains(&job["status"].as_str().unwrap()) {
            return job;
        }
        assert!(
            Instant::now() < deadline,
            "Optional preparation did not reach a checkpoint"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn requests_default_to_no_download_and_reject_urls_plugins_and_unknown_tools() {
    for tool in ["audio_codecs", "vst3_host"] {
        let request: PrepareToolRequest = serde_json::from_value(json!({"tool":tool})).unwrap();
        assert!(!request.allow_download);
        assert!(serde_json::from_value::<PrepareToolRequest>(json!({"tool":tool,"allow_download":true,"url":"https://untrusted.invalid/plugin.zip"})).is_err());
    }
    assert!(
        serde_json::from_value::<PrepareToolRequest>(
            json!({"tool":"arbitrary_plugin","allow_download":true})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<PrepareToolRequest>(
            json!({"tool":"mcp_bridge","allow_download":true})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<PrepareToolRequest>(
            json!({"tool":"audio_codecs","allow_download":"true"})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<ToolStatusRequest>(json!({}))
            .unwrap()
            .tool
            .is_none()
    );
    assert!(serde_json::from_value::<ToolStatusRequest>(json!({"allow_download":true})).is_err());
}

#[test]
fn absent_consent_rejects_before_job_reservation_and_setup_entry() {
    let root = tempfile::tempdir().unwrap();
    let engine = engine(root.path());
    let before = engine.session().unwrap();
    for tool in [OptionalTool::AudioCodecs, OptionalTool::Vst3Host] {
        let entered = Arc::new(AtomicUsize::new(0));
        let calls = entered.clone();
        let error = start_with(&engine, tool, false, move |_, _, _| {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok(json!({}))
        })
        .unwrap_err();
        assert!(error.to_string().contains("allow_download:true"));
        assert_eq!(entered.load(Ordering::Relaxed), 0);
        assert_eq!(engine.lock().unwrap().sequence, 0);
        assert_eq!(engine.session().unwrap(), before);
    }
}

#[test]
fn explicit_preparation_is_one_job_with_bounded_progress_and_no_session_mutation() {
    let root = tempfile::tempdir().unwrap();
    let engine = engine(root.path());
    engine.lock().unwrap().session.settings.intensity = 0.37;
    let before = engine.session().unwrap();
    let (entered, waiting) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let job = start_with(
        &engine,
        OptionalTool::AudioCodecs,
        true,
        move |tool, cancel, progress| {
            assert_eq!(tool, OptionalTool::AudioCodecs);
            assert!(!cancel.load(Ordering::Relaxed));
            progress(1., "Fake capabilities checked; finishing preparation");
            entered.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(json!({"tool":tool,"available":true}))
        },
    )
    .unwrap();
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    let id = job["job_id"].as_str().unwrap();
    let active = engine.job(id).unwrap();
    assert!(active["progress"].as_f64().unwrap() < 1.);
    assert_eq!(active["kind"], "prepare_optional_tool");
    assert!(
        start_with(&engine, OptionalTool::Vst3Host, true, |_, _, _| panic!(
            "Overlapping setup"
        ))
        .is_err()
    );
    release.send(()).unwrap();
    let done = terminal(&engine, id);
    assert_eq!(done["status"], "completed");
    assert_eq!(done["progress"], 1.);
    assert_eq!(done["result"]["tool"], "audio_codecs");
    assert_eq!(engine.session().unwrap(), before);
}

#[test]
fn cancellation_preserves_settings_and_never_continues_even_if_cache_finishes() {
    let root = tempfile::tempdir().unwrap();
    let engine = engine(root.path());
    engine.lock().unwrap().session.settings.intensity = 0.42;
    let before = engine.session().unwrap();
    let cached = Arc::new(AtomicBool::new(false));
    let installed = cached.clone();
    let (entered, waiting) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let job = start_with(
        &engine,
        OptionalTool::Vst3Host,
        true,
        move |_, token, progress| {
            progress(0.7, "Fake helper staged");
            entered.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(token.load(Ordering::Relaxed));
            installed.store(true, Ordering::Relaxed);
            Ok(json!({"available":true}))
        },
    )
    .unwrap();
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    let id = job["job_id"].as_str().unwrap();
    engine.cancel(id).unwrap();
    release.send(()).unwrap();
    let done = terminal(&engine, id);
    assert!(cached.load(Ordering::Relaxed));
    assert_eq!(done["status"], "cancelled");
    assert!(done["result"].is_null());
    assert!(done["progress"].as_f64().unwrap() < 1.);
    assert_eq!(
        engine.lock().unwrap().sequence,
        1,
        "Setup must not auto-submit audio/plugin work"
    );
    assert_eq!(engine.session().unwrap(), before);
}

#[test]
fn setup_failure_reports_cause_and_preserves_session() {
    let root = tempfile::tempdir().unwrap();
    let engine = engine(root.path());
    let before = engine.session().unwrap();
    for tool in [OptionalTool::AudioCodecs, OptionalTool::Vst3Host] {
        let job = start_with(&engine, tool, true, |_, _, _| {
            anyhow::bail!("No compatible official bundle")
        })
        .unwrap();
        let done = terminal(&engine, job["job_id"].as_str().unwrap());
        assert_eq!(done["status"], "failed");
        assert!(
            done["error"]
                .as_str()
                .unwrap()
                .contains("No compatible official bundle")
        );
        assert!(done["result"].is_null());
        assert_eq!(engine.session().unwrap(), before);
    }
}

#[test]
fn detection_does_not_invent_verification_or_installed_component_versions() {
    let detected = observation(
        OptionalTool::Vst3Host,
        true,
        "detected_only",
        None,
        true,
        "bundled",
    );
    assert!(detected["hashes_verified"].is_null());
    assert!(detected["component_version"].is_null());
    assert!(detected["installed_application_version"].is_null());
    let verified = observation(
        OptionalTool::Vst3Host,
        true,
        "private_executable_hash_verified",
        Some(true),
        true,
        "private",
    );
    assert_eq!(verified["hashes_verified"], true);
    assert!(verified["component_version"].is_null());
}

#[test]
fn discovery_and_help_keep_numeric_limits_and_explicit_setup_visible() {
    let root = tempfile::tempdir().unwrap();
    let engine = engine(root.path());
    let capabilities = crate::capabilities::describe(&engine);
    let guide = crate::help::get(crate::help::Topic::QuickStart);
    for value in [
        &capabilities["optional_tools"],
        &guide["optional_tools"],
        &help(),
    ] {
        assert_eq!(value["allow_download_default"], false);
        assert_eq!(
            value["audio_codecs"]["max_download_bytes"]["windows_x86_64"],
            192_u64 * 1024 * 1024
        );
        assert_eq!(
            value["audio_codecs"]["max_download_bytes"]["linux_x86_64_gnu"],
            160_u64 * 1024 * 1024
        );
        assert_eq!(
            value["vst3_host"]["max_download_bytes"],
            192_u64 * 1024 * 1024
        );
        assert_eq!(
            value["vst3_host"]["preparation_mode"],
            "explicit_private_recovery"
        );
        assert_eq!(value["vst3_host"]["reuses_bundled_host"], false);
        assert!(value["privacy"].as_str().unwrap().contains("not total RAM"));
    }
    assert!(
        guide["optional_tools"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| step["tool"] == "prepare_optional_tool")
    );
    let instructions = crate::help::INSTRUCTIONS;
    assert!(
        instructions.contains("allow_download:true") && instructions.contains("owner agreement")
    );
}
