//! Shared bounded process supervision for codecs, model runtimes and plugin workers.
use anyhow::{Context, Result, ensure};
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub fn read_stdout(
    mut command: Command,
    max_bytes: usize,
    timeout: Duration,
    cancel: Arc<AtomicBool>,
    mut consume: impl FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    let mut stderr = tempfile::tempfile()?;
    crate::process_group::WorkerGroup::configure(&mut command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr.try_clone()?))
        .spawn()
        .context("Cannot start the required local worker; check its installation")?;
    let group = match crate::process_group::WorkerGroup::attach(&child) {
        Ok(group) => Arc::new(group),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let mut stdout = child
        .stdout
        .take()
        .context("Decoder stdout is unavailable")?;
    let child = Arc::new(Mutex::new(child));
    let stopped = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    let logs_exceeded = Arc::new(AtomicBool::new(false));
    let stderr_monitor = stderr.try_clone()?;
    let watcher = {
        let child = child.clone();
        let stopped = stopped.clone();
        let timed_out = timed_out.clone();
        let cancel = cancel.clone();
        let group = group.clone();
        let logs_exceeded = logs_exceeded.clone();
        thread::spawn(move || {
            let start = Instant::now();
            while !stopped.load(Ordering::Relaxed) {
                let too_much_log = stderr_monitor
                    .metadata()
                    .is_ok_and(|m| m.len() > 8 * 1024 * 1024);
                if cancel.load(Ordering::Relaxed) || start.elapsed() > timeout || too_much_log {
                    timed_out.store(start.elapsed() > timeout, Ordering::Relaxed);
                    logs_exceeded.store(too_much_log, Ordering::Relaxed);
                    group.kill();
                    if let Ok(mut child) = child.lock() {
                        let _ = child.kill();
                    }
                    break;
                }
                thread::sleep(Duration::from_millis(25));
            }
        })
    };
    let reading = (|| -> Result<()> {
        let mut chunk = [0_u8; 65536];
        let mut total = 0_usize;
        loop {
            let count = stdout.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count)
                .context("Decoded audio size overflow")?;
            ensure!(
                total <= max_bytes,
                "Local process output exceeds the configured byte limit"
            );
            ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
            consume(&chunk[..count])?;
        }
        Ok(())
    })();
    if reading.is_err() {
        group.kill();
        let _ = child.lock().map(|mut c| c.kill());
    }
    // Never hold the child mutex in wait(): the monitor must still be able to kill
    // a process that closes stdout and then stalls.
    let status = loop {
        let polled = child
            .lock()
            .map_err(|_| anyhow::anyhow!("Decoder state unavailable"))?
            .try_wait();
        match polled {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => break Err(error),
        }
    };
    stopped.store(true, Ordering::Relaxed);
    let joined = watcher.join();
    ensure!(
        joined.is_ok(),
        "Decoder cancellation monitor stopped unexpectedly"
    );
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    ensure!(
        !logs_exceeded.load(Ordering::Relaxed),
        "Local worker diagnostic log exceeded 8 MiB"
    );
    ensure!(
        !timed_out.load(Ordering::Relaxed),
        "Decoder exceeded the configured timeout"
    );
    reading?;
    use std::io::{Seek, SeekFrom};
    let tail = stderr.metadata()?.len().saturating_sub(8192);
    stderr.seek(SeekFrom::Start(tail))?;
    let mut error = Vec::new();
    stderr.take(8192).read_to_end(&mut error)?;
    ensure!(
        status?.success(),
        "Decoder failed: {}",
        String::from_utf8_lossy(&error).trim()
    );
    Ok(())
}
