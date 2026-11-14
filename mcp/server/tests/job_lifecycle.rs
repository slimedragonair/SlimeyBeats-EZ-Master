use serde_json::json;
use slimeybeats_mcp::{
    arguments::Arguments,
    jobs::{Change, Product},
    state::Engine,
};
use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

fn engine(dir: &std::path::Path) -> Arc<Engine> {
    Engine::new(&Arguments {
        input_root: vec![dir.into()],
        output_root: dir.join("exports"),
        max_decoded_mib: 8,
        decode_timeout_seconds: 5,
        desktop_exe: None,
        studio_data: None,
        plugin_root: vec![],
        print_config: false,
    })
    .unwrap()
}
fn terminal(engine: &Engine, id: &str) -> serde_json::Value {
    let start = Instant::now();
    loop {
        let job = engine.job(id).unwrap();
        if ["completed", "failed", "cancelled"].contains(&job["status"].as_str().unwrap()) {
            return job;
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn clear_cancels_pending_changes_without_permitting_overlapping_work() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    let (send, receive) = mpsc::channel();
    let (ready, waiting) = mpsc::channel();
    let job = engine
        .start(
            "blocked",
            |_| Ok(()),
            move |ctx| {
                ctx.progress(1., "Finishing");
                ready.send(()).unwrap();
                receive.recv().unwrap();
                Ok(Product {
                    result: json!({"wrong":true}),
                    change: Change::Reference(None),
                    committed: false,
                })
            },
        )
        .unwrap();
    let id = job["job_id"].as_str().unwrap();
    waiting.recv().unwrap();
    assert!(engine.job(id).unwrap()["progress"].as_f64().unwrap() < 1.);
    engine.clear().unwrap();
    assert!(
        engine
            .start("overlap", |_| Ok(()), |_| Ok(Product::value(json!({}))))
            .is_err()
    );
    send.send(()).unwrap();
    assert_eq!(terminal(&engine, id)["status"], "cancelled");
    assert!(engine.session().unwrap()["source"].is_null());
}

#[test]
fn cancellation_cannot_hide_already_committed_exports_and_history_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    let (send, receive) = mpsc::channel();
    let job = engine
        .start(
            "export",
            |_| Ok(()),
            move |_| {
                receive.recv().unwrap();
                Ok(Product {
                    result: json!({"exports":["saved.wav"]}),
                    change: Change::None,
                    committed: true,
                })
            },
        )
        .unwrap();
    let id = job["job_id"].as_str().unwrap();
    engine.cancel(id).unwrap();
    send.send(()).unwrap();
    let finished = terminal(&engine, id);
    assert_eq!(finished["status"], "completed");
    assert_eq!(finished["result"]["exports"][0], "saved.wav");
    for _ in 0..65 {
        let job = engine
            .start("short", |_| Ok(()), |_| Ok(Product::value(json!({}))))
            .unwrap();
        terminal(&engine, job["job_id"].as_str().unwrap());
    }
    assert!(engine.job(id).is_err());
}

#[test]
fn failures_preserve_session_and_return_explicit_errors() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    let before = engine.session().unwrap();
    let job = engine
        .start("failure", |_| Ok(()), |_| anyhow::bail!("broken codec"))
        .unwrap();
    let failed = terminal(&engine, job["job_id"].as_str().unwrap());
    assert_eq!(failed["status"], "failed");
    assert!(failed["error"].as_str().unwrap().contains("broken codec"));
    assert_eq!(before, engine.session().unwrap());
}
