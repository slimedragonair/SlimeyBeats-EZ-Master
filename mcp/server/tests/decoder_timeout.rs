use slimeybeats_mcp::codec_process::read_stdout;
use std::{
    process::Command,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

#[test]
fn a_child_that_closes_stdout_but_hangs_is_still_timed_out() {
    let python = if cfg!(windows) { "python" } else { "python3" };
    // The stdlib protocol validator and CI already require Python. No shell is involved.
    let mut command = Command::new(python);
    command.args(["-c", "import os,time; os.close(1); time.sleep(30)"]);
    let start = Instant::now();
    let result = read_stdout(
        command,
        1024,
        Duration::from_millis(150),
        Arc::new(AtomicBool::new(false)),
        |_| Ok(()),
    );
    assert!(result.unwrap_err().to_string().contains("timeout"));
    assert!(start.elapsed() < Duration::from_secs(5));
}
