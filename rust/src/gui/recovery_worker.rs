//! One disk worker owns a window's recovery lock. The renderer only sends snapshots.
use super::{
    recovery_capture::Capture,
    recovery_restore::{self, Prepared},
};
use crate::recovery_store::{self, CheckpointInfo, CheckpointRef, Listing, Snapshot, Store};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

pub(super) enum Work {
    Save(u64, Capture),
    History,
    Inspect(CheckpointRef),
    Restore(CheckpointRef),
    Delete(String),
    ClearCurrent(Option<String>),
}
pub(super) enum Outcome {
    Saved(u64, CheckpointInfo),
    History(Listing),
    Inspected(CheckpointRef, serde_json::Value),
    Restored(Box<Prepared>),
    Deleted(String),
    CurrentCleared(Option<String>),
}
pub(super) enum Command {
    Run(u64, Work, Arc<AtomicBool>),
    Stop {
        clean: bool,
        reply: mpsc::Sender<Result<(), String>>,
    },
}
pub(super) struct Completed {
    pub id: u64,
    pub result: Result<Outcome, String>,
}
pub(super) struct Worker {
    pub sender: mpsc::Sender<Command>,
    pub receiver: mpsc::Receiver<Completed>,
}

pub(super) fn start(root: PathBuf, ctx: egui::Context) -> std::io::Result<Worker> {
    let (sender, commands) = mpsc::channel();
    let (events, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("slimey-recovery".into())
        .spawn(move || {
            let mut store: Option<Store> = None;
            // Retain failed deletion targets after releasing their Store lease.
            // Retry must remove the same runs even when last_saved is cleared.
            let mut pending_clear_runs: Vec<String> = Vec::new();
            while let Ok(command) = commands.recv() {
                match command {
                    Command::Stop { clean, reply } => {
                        let result = if clean && let Some(store) = &mut store {
                            store.finish(&|| false).map_err(|e| {
                                format!("Recovery clean-exit marker could not be saved: {e:#}")
                            })
                        } else {
                            Ok(())
                        };
                        drop(store);
                        let _ = reply.send(result);
                        break;
                    }
                    Command::Run(id, work, cancel) => {
                        let cancelled = || cancel.load(Ordering::Relaxed);
                        let result = (|| -> anyhow::Result<Outcome> {
                            anyhow::ensure!(!cancelled(), "Cancelled");
                            Ok(match work {
                                Work::Save(key, capture) => {
                                    if store.is_none() {
                                        store = Some(Store::open(
                                            &root,
                                            recovery_store::DEFAULT_BUDGET_BYTES,
                                            &cancelled,
                                        )?);
                                    }
                                    let saved = store.as_mut().unwrap().save(
                                        &Snapshot {
                                            payload: capture.payload,
                                            audio: capture.audio,
                                        },
                                        &cancelled,
                                    )?;
                                    Outcome::Saved(key, saved)
                                }
                                Work::History => Outcome::History(recovery_store::list_history(
                                    &root, &cancelled,
                                )?),
                                Work::Inspect(key) => {
                                    let payload = recovery_store::inspect(&root, &key, &cancelled)?;
                                    Outcome::Inspected(key, payload)
                                }
                                Work::Restore(key) => {
                                    // Protect the replaced draft as a separately addressable
                                    // closed session, outside subsequent point retention.
                                    if let Some(mut backup) = store.take() {
                                        backup.finish(&cancelled)?;
                                    }
                                    let saved = recovery_store::load(&root, &key, &cancelled)?;
                                    let prepared =
                                        recovery_restore::prepare(saved.payload, saved.audio)?;
                                    anyhow::ensure!(!cancelled(), "Cancelled");
                                    Outcome::Restored(Box::new(prepared))
                                }
                                Work::Delete(id) => {
                                    recovery_store::discard_run(&root, &id, &cancelled)?;
                                    Outcome::Deleted(id)
                                }
                                Work::ClearCurrent(fallback) => {
                                    let id =
                                        store.as_ref().map(|s| s.run_id().to_owned()).or(fallback);
                                    if let Some(id) = &id
                                        && !pending_clear_runs.contains(id)
                                    {
                                        pending_clear_runs.push(id.clone());
                                    }
                                    drop(store.take());
                                    for id in &pending_clear_runs {
                                        recovery_store::discard_run(&root, id, &cancelled)?;
                                    }
                                    // A previous attempt may have removed its run before
                                    // cleanup was cancelled. Retry owned orphan cleanup.
                                    recovery_store::cleanup_unused(&root, &cancelled)?;
                                    pending_clear_runs.clear();
                                    Outcome::CurrentCleared(id)
                                }
                            })
                        })()
                        .map_err(|error| format!("{error:#}"));
                        let _ = events.send(Completed { id, result });
                        ctx.request_repaint();
                    }
                }
            }
        })?;
    Ok(Worker { sender, receiver })
}
pub(super) fn stop(worker: Option<Worker>, clean: bool, cancel: &AtomicBool) {
    cancel.store(true, Ordering::Relaxed);
    if let Some(worker) = worker {
        let (reply, done) = mpsc::channel();
        if worker.sender.send(Command::Stop { clean, reply }).is_ok() {
            // The visible close flow waits for media copies. Final marker/lock
            // release is normally tiny; forced shutdown still leaves a valid prior point.
            match done.recv_timeout(Duration::from_secs(2)) {
                Ok(Err(error)) => eprintln!("{error}. Earlier committed copies remain available."),
                Err(error) => eprintln!(
                    "Recovery shutdown did not complete: {error}. The previous recovery point is retained."
                ),
                Ok(Ok(())) => {}
            }
        }
    }
}
