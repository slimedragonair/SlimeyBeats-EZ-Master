//! One explicitly requested separation owns setup and inference in the same job.
use anyhow::{Context, Result, ensure};
use std::{
    cell::Cell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const SETUP_SHARE: f32 = 0.4;
// Rendering and measuring the usable preview still follow inference.
const SEPARATION_END: f32 = 0.94;

pub(super) fn run<T>(
    needs_setup: bool,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(f32, &str),
    install: impl FnOnce(&dyn Fn(f32, &str), Arc<AtomicBool>) -> Result<()>,
    separate: impl FnOnce(&dyn Fn(f32, &str), Arc<AtomicBool>) -> Result<T>,
) -> Result<T> {
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    let last = Cell::new(0_f32);
    let report = |fraction: f32, message: &str| {
        let fraction = fraction.clamp(0., SEPARATION_END).max(last.get());
        last.set(fraction);
        progress(fraction, message);
    };
    let start = if needs_setup {
        report(
            0.,
            "One-time stem setup; separation will follow automatically. Cancel stops this job.",
        );
        ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
        install(
            &|fraction, message| report(fraction.clamp(0., 1.) * SETUP_SHARE, message),
            cancel.clone(),
        )
        .context("Stem setup stopped; retry Setup and separate or use Engine maintenance")?;
        // A successfully cached installation must never resume a cancelled request.
        ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
        SETUP_SHARE
    } else {
        0.
    };
    report(start, "Separating the source locally");
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    let result = separate(
        &|fraction, message| {
            report(
                start + fraction.clamp(0., 1.) * (SEPARATION_END - start),
                message,
            )
        },
        cancel.clone(),
    )?;
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    Ok(result)
}
