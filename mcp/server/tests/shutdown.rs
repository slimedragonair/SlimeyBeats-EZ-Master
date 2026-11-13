use slimeybeats_mcp::{arguments::Arguments, jobs::Product, state::Engine};
use std::time::Duration;

#[test]
fn closing_the_connection_waits_for_the_cancelled_worker_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::new(&Arguments {
        input_root: vec![dir.path().into()],
        output_root: dir.path().join("output"),
        max_decoded_mib: 8,
        decode_timeout_seconds: 5,
        desktop_exe: None,
        studio_data: None,
        plugin_root: vec![],
        print_config: false,
    })
    .unwrap();
    let submitted = engine
        .start(
            "cooperative",
            |_| Ok(()),
            |ctx| {
                while !ctx.cancelled() {
                    std::thread::sleep(Duration::from_millis(2));
                }
                ctx.check()?;
                Ok(Product::value(serde_json::json!({})))
            },
        )
        .unwrap();
    engine.shutdown();
    assert_eq!(
        engine.job(submitted["job_id"].as_str().unwrap()).unwrap()["status"],
        "cancelled"
    );
}
