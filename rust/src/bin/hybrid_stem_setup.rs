//! Operator-only setup for an explicitly selected private model runtime.
use anyhow::{Result, ensure};
use clap::Parser;
use mastercraft::studio_native::runtime::RuntimePaths;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::{Duration, Instant},
};

#[derive(Parser)]
struct Args {
    /// Dedicated absolute directory; no default desktop installation is used.
    #[arg(long)]
    base: PathBuf,
    /// Read readiness only, without downloads, installation, or model hashing.
    #[arg(long)]
    probe: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(args.base.is_absolute(), "--base must be an absolute path");
    ensure!(
        args.base.parent().is_some(),
        "--base must not be a filesystem root"
    );
    let paths = RuntimePaths { base: args.base };
    let last = Mutex::new((Instant::now() - Duration::from_secs(1), -1_i32));
    let status = if args.probe {
        paths.status()
    } else {
        paths.install(
            &|fraction, message| {
                let percent = (fraction.clamp(0., 1.) * 100.) as i32;
                let mut previous = last.lock().expect("progress lock");
                if previous.0.elapsed() >= Duration::from_millis(500) || percent != previous.1 {
                    eprintln!("{}", json!({"event":"progress", "fraction":fraction.clamp(0., 1.), "message":message.chars().take(240).collect::<String>()}));
                    *previous = (Instant::now(), percent);
                }
            },
            Arc::new(AtomicBool::new(false)),
        )?
    };
    println!(
        "{}",
        json!({"runtime":status, "base":paths.base, "python":paths.python(), "models":paths.models(), "receipt":paths.receipt()})
    );
    ensure!(
        paths.is_ready(),
        "Runtime is not ready; run setup without --probe"
    );
    Ok(())
}
