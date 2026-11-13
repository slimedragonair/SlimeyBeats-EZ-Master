//! Bounded, cancellable local decoder execution; no shell or network transport.
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
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr.try_clone()?))
        .spawn()
        .context("Install FFmpeg and ffprobe on PATH for compressed audio")?;
    let mut stdout = child
        .stdout
        .take()
        .context("Decoder stdout is unavailable")?;
    let child = Arc::new(Mutex::new(child));
    let stopped = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    let watcher = {
        let child = child.clone();
        let stopped = stopped.clone();
        let timed_out = timed_out.clone();
        let cancel = cancel.clone();
        thread::spawn(move || {
            let start = Instant::now();
            while !stopped.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) || start.elapsed() > timeout {
                    timed_out.store(start.elapsed() > timeout, Ordering::Relaxed);
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
                "Decoded audio exceeds the configured memory budget"
            );
            ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
            consume(&chunk[..count])?;
        }
        Ok(())
    })();
    if reading.is_err() {
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
        !timed_out.load(Ordering::Relaxed),
        "Decoder exceeded the configured timeout"
    );
    reading?;
    use std::io::{Seek, SeekFrom};
    stderr.seek(SeekFrom::Start(0))?;
    let mut error = Vec::new();
    stderr.take(8192).read_to_end(&mut error)?;
    ensure!(
        status?.success(),
        "Decoder failed: {}",
        String::from_utf8_lossy(&error).trim()
    );
    Ok(())
}
