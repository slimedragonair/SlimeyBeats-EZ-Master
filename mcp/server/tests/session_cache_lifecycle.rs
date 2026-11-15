//! Real stdio/EOF/forced-termination checks, using generated audio and private output.
use mastercraft::{session_document::SessionDocument, studio_native::session::StudioDocument};
use serde_json::json;
use std::{collections::BTreeSet, path::Path};

#[path = "support/stdio_server.rs"]
mod stdio_server;
use stdio_server::Server;

fn scratch(output: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(output)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".slimeybeats-mcp-")
        })
        .collect()
}

fn fixture(path: &Path, frequency: f32) {
    let mut wav = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 16000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..32000 {
        let sample = 0.1 * (frame as f32 * frequency * std::f32::consts::TAU / 16000.).sin();
        wav.write_sample(sample).unwrap();
        wav.write_sample(sample * 0.8).unwrap();
    }
    wav.finalize().unwrap();
}

#[test]
fn repeated_read_only_connections_leave_no_scratch_on_eof_or_forced_termination() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("exports");
    let mut ids = BTreeSet::new();
    for index in 0..12 {
        let mut server = Server::start(root.path(), &output, None);
        assert!(
            !server.request("tools/list", json!({}))["tools"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        server.call("get_capabilities", json!({}));
        server.call("get_help", json!({}));
        server.call("get_studio_session", json!({}));
        let session = server.call("get_session", json!({}));
        assert!(ids.insert(session["connection_id"].as_str().unwrap().to_owned()));
        assert_eq!(std::fs::read_dir(&output).unwrap().count(), 0);
        if index % 2 == 0 {
            server.close();
        } else {
            server.kill();
        }
        assert_eq!(std::fs::read_dir(&output).unwrap().count(), 0);
    }
}

fn durable_workflow(input: &Path, output: &Path) {
    fixture(&input.join("vocals.wav"), 220.);
    fixture(&input.join("instrumental.wav"), 1100.);
    let original = std::fs::read(input.join("vocals.wav")).unwrap();
    // A non-executable trusted fixture exercises durable handoff publication
    // and its explicit launch warning without opening a user's desktop app.
    let desktop = input.join("not-a-desktop.exe");
    std::fs::write(&desktop, b"not an executable").unwrap();
    let mut server = Server::start(input, output, Some(&desktop));
    let loaded = server.run("load_audio", json!({"library":0,"path":"vocals.wav"}));
    server.run("analyze_audio", json!({"source_id":loaded["source_id"]}));
    assert!(
        scratch(output).is_empty(),
        "Loading/analysis must not allocate previews"
    );
    let imported = server.run(
        "import_stems",
        json!({"files":[
            {"kind":"vocals","library":0,"path":"vocals.wav"},
            {"kind":"instrumental","library":0,"path":"instrumental.wav"}
        ]}),
    );
    let preview = &imported["preview"];
    let temporary = output.join(preview["relative_preview_path"].as_str().unwrap());
    assert!(temporary.is_file());
    assert_eq!(
        scratch(output),
        vec![temporary.parent().unwrap().to_path_buf()]
    );
    let promoted = server.run(
        "use_studio_for_mastering",
        json!({
            "studio_preview_id":preview["studio_preview_id"]
        }),
    );
    let master = server.run("master_preview", json!({"source_id":promoted["source_id"]}));
    let args = json!({"preview_id":master["preview_id"],"name":"Saved master","format":"wav32f"});
    let first = server.run("export_audio", args.clone());
    let exported = output.join(first["exports"][0]["path"].as_str().unwrap());
    let first_bytes = std::fs::read(&exported).unwrap();
    let second = server.run("export_audio", args);
    assert_ne!(first["exports"][0]["path"], second["exports"][0]["path"]);
    let project = server.run("save_studio_project", json!({}));
    let project = output.join(project["project_path"].as_str().unwrap());
    let handoff = server.run("open_in_desktop", json!({}));
    assert_eq!(handoff["desktop_launched"], false);
    assert!(handoff["warning"].as_str().is_some());
    let handoff = output.join(handoff["session_path"].as_str().unwrap());
    // EOF must cancel/join pending audio work before releasing the shared cache.
    server.call(
        "apply_eq",
        json!({"preview_id":master["preview_id"],
        "options":{"output_gain_db":-1.,"hold_loudness":false}}),
    );
    server.close();
    assert!(scratch(output).is_empty());
    assert_eq!(std::fs::read(&exported).unwrap(), first_bytes);
    assert_eq!(std::fs::read(input.join("vocals.wav")).unwrap(), original);
    std::fs::remove_file(input.join("vocals.wav")).unwrap();
    std::fs::remove_file(input.join("instrumental.wav")).unwrap();
    let saved = StudioDocument::load(&project).unwrap();
    assert_eq!(saved.stems.unwrap().tracks.len(), 2);
    let saved = SessionDocument::load(&handoff).unwrap();
    saved.validate().unwrap();
    assert!(saved.source.path.is_file());
    assert!(!saved.source.path.starts_with(temporary.parent().unwrap()));
    assert!(std::fs::read_dir(output).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".mastercraft-")
    }));
}

#[test]
fn eof_cleans_preview_scratch_but_preserves_exports_projects_and_generated_handoff() {
    let root = tempfile::tempdir().unwrap();
    durable_workflow(root.path(), &root.path().join("exports"));
}

#[test]
fn reconnect_does_not_sweep_audio_scratch_left_by_a_forced_processing_termination() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("exports");
    fixture(&root.path().join("input.wav"), 220.);
    let mut server = Server::start(root.path(), &output, None);
    let source = server.run("load_audio", json!({"library":0,"path":"input.wav"}));
    let master = server.run("master_preview", json!({"source_id":source["source_id"]}));
    let preview = output.join(master["relative_preview_path"].as_str().unwrap());
    let preview_bytes = std::fs::read(&preview).unwrap();
    let saved = server.run(
        "export_audio",
        json!({
            "preview_id":master["preview_id"],"name":"Keep this export","format":"wav32f"
        }),
    );
    let saved = output.join(saved["exports"][0]["path"].as_str().unwrap());
    let saved_bytes = std::fs::read(&saved).unwrap();
    server.kill();
    // Hard termination cannot run Drop. A new connection owns none of these
    // files and must neither sweep retained audio nor allocate an empty sibling.
    let mut reconnected = Server::start(root.path(), &output, None);
    reconnected.request("tools/list", json!({}));
    reconnected.call("get_session", json!({}));
    reconnected.close();
    assert_eq!(
        scratch(&output),
        vec![preview.parent().unwrap().to_path_buf()]
    );
    assert_eq!(std::fs::read(&preview).unwrap(), preview_bytes);
    assert_eq!(std::fs::read(saved).unwrap(), saved_bytes);
}

#[test]
#[ignore = "Set SLIMEYBEATS_TEST_OUTPUT_ROOT to a writable folder on a different volume"]
fn configured_other_volume_preserves_atomic_exports_and_self_contained_handoff() {
    let input = tempfile::tempdir().unwrap();
    let parent = std::env::var_os("SLIMEYBEATS_TEST_OUTPUT_ROOT")
        .expect("Set SLIMEYBEATS_TEST_OUTPUT_ROOT to a different-volume test folder");
    let output = tempfile::Builder::new()
        .prefix("SlimeyBeats cross-volume test-")
        .tempdir_in(parent)
        .unwrap();
    #[cfg(windows)]
    assert_ne!(
        input.path().canonicalize().unwrap().components().next(),
        output.path().canonicalize().unwrap().components().next(),
        "Input/temp and output must use different drive prefixes"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_ne!(
            std::fs::metadata(input.path()).unwrap().dev(),
            std::fs::metadata(output.path()).unwrap().dev(),
            "Input/temp and output must use different mounted filesystems"
        );
    }
    durable_workflow(input.path(), output.path());
}
