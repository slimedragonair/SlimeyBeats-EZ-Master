//! Disposable launch-contract probe; it opens no window or audio device.
use std::{fs, io::Write};

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let executable = std::env::current_exe().unwrap();
    if args == ["--version"] {
        println!("slimeybeats-ez-master-live 0.5.0-beta.3 revision 1501");
        return;
    }
    if args == ["--studio-project-capabilities"] {
        let mut log = fs::OpenOptions::new().append(true).create(true)
            .open(executable.with_extension("capability-probes.jsonl")).unwrap();
        writeln!(log, "{{\"flag\":\"--studio-project-capabilities\"}}").unwrap();
        log.sync_all().unwrap();
        let mode = fs::read_to_string(executable.with_extension("mode")).unwrap_or_default();
        match mode.trim() {
            "old-same-version" => { eprintln!("Unknown option --studio-project-capabilities (0.5.0-beta.3 revision 1501)"); std::process::exit(2); }
            "incomplete" => println!(r#"{{"format":"slimeybeats-desktop-project-capabilities","version":1,"engine_version":"0.5.0-beta.3","studio_project_versions":[1,2]}}"#),
            "malformed" => println!("not capability JSON"),
            "timeout" => std::thread::sleep(std::time::Duration::from_secs(30)),
            "oversized" => println!("{}", "x".repeat(16384)),
            _ => println!(r#"{{"format":"slimeybeats-desktop-project-capabilities","version":1,"engine_version":"0.5.0-beta.3","studio_project_versions":[1,2],"arrangement_live_editing":true,"autoplay_on_project_open":false}}"#),
        }
        return;
    }
    assert_eq!(args.len(), 2);
    assert_eq!(args[0], "--studio-session");
    let manifest = fs::read_to_string(&args[1]).expect("saved project manifest");
    let version_two = manifest.contains("\"version\": 2");
    assert!(version_two && manifest.contains("slimeybeats-studio"));
    let output = executable.with_extension("launches.jsonl");
    let mut file = fs::OpenOptions::new().append(true).create(true).open(output).unwrap();
    writeln!(file, "{{\"flag\":{:?},\"manifest\":{:?},\"canonical_v2_header\":{}}}", args[0], args[1], version_two).unwrap();
    file.sync_all().unwrap();
}
