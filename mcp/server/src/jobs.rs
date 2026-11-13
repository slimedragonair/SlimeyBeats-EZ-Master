use crate::state::{Engine, Input, Master, Reference, Snapshot};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

const HISTORY_LIMIT: usize = 64;

pub struct ActiveJob {
    pub id: String,
    pub cancel: Arc<AtomicBool>,
}

#[derive(Clone, Serialize)]
pub struct JobRecord {
    pub job_id: String,
    pub kind: String,
    pub status: String,
    pub progress: f32,
    pub message: String,
    pub result: Option<Value>,
    pub error: Option<String>,
    #[serde(skip)]
    pub started: Instant,
    pub elapsed_seconds: Option<f64>,
}

pub struct ContextData {
    pub engine: Arc<Engine>,
    pub id: String,
    pub snapshot: Snapshot,
    pub cancel: Arc<AtomicBool>,
}
impl ContextData {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    pub fn check(&self) -> Result<()> {
        ensure!(!self.cancelled(), "Cancelled");
        Ok(())
    }
    pub fn progress(&self, fraction: f32, message: &str) {
        if let Ok(mut store) = self.engine.lock()
            && let Some(job) = store.jobs.get_mut(&self.id)
        {
            // Only the commit below can publish 100% and a completed status.
            job.progress = fraction.clamp(0., 0.99);
            if job.status != "cancellation_requested" {
                job.message = message.to_string();
            }
        }
    }
}

pub enum Change {
    None,
    Source(Arc<Input>),
    Reference(Option<Arc<Reference>>),
    Master(Arc<Master>),
}
pub struct Product {
    pub result: Value,
    pub change: Change,
    /// Durable exports or an already launched desktop cannot be undone by a late cancellation.
    pub committed: bool,
}
impl Product {
    pub fn value(result: Value) -> Self {
        Self {
            result,
            change: Change::None,
            committed: false,
        }
    }
}

impl Engine {
    pub fn start(
        self: &Arc<Self>,
        kind: &str,
        validate: impl FnOnce(&Snapshot) -> Result<()>,
        work: impl FnOnce(ContextData) -> Result<Product> + Send + 'static,
    ) -> Result<Value> {
        let mut store = self.lock()?;
        ensure!(
            store.active.is_none(),
            "Another audio job is active. Poll get_job_status or cancel it first"
        );
        validate(&store.session)?;
        while store.order.len() >= HISTORY_LIMIT {
            if let Some(old) = store.order.pop_front() {
                store.jobs.remove(&old);
            }
        }
        store.sequence += 1;
        let id = format!("job-{}", store.sequence);
        let cancel = Arc::new(AtomicBool::new(false));
        let generation = store.generation;
        let context = ContextData {
            engine: self.clone(),
            id: id.clone(),
            snapshot: store.session.clone(),
            cancel: cancel.clone(),
        };
        store.active = Some(ActiveJob {
            id: id.clone(),
            cancel: cancel.clone(),
        });
        store.order.push_back(id.clone());
        store.jobs.insert(
            id.clone(),
            JobRecord {
                job_id: id.clone(),
                kind: kind.into(),
                status: "running".into(),
                progress: 0.,
                message: "Starting local processing".into(),
                result: None,
                error: None,
                started: Instant::now(),
                elapsed_seconds: None,
            },
        );
        drop(store);
        let engine = self.clone();
        let job_id = id.clone();
        std::thread::spawn(move || {
            let outcome = catch_unwind(AssertUnwindSafe(|| work(context))).unwrap_or_else(|_| {
                Err(anyhow::anyhow!(
                    "Audio job panicked; original files were preserved"
                ))
            });
            let Ok(mut store) = engine.lock() else {
                eprintln!("SlimeyBeats: job state unavailable");
                return;
            };
            let late_cancel = cancel.load(Ordering::Relaxed) || store.generation != generation;
            let committed = outcome.as_ref().is_ok_and(|p| p.committed);
            let (status, result, error) = match outcome {
                Ok(product) if !late_cancel || committed => {
                    if !late_cancel {
                        match product.change {
                            Change::None => {}
                            Change::Source(source) => {
                                let settings = mastercraft::config::Settings {
                                    out_dir: engine.policy.output.clone(),
                                    ..Default::default()
                                };
                                store.session = Snapshot {
                                    source: Some(source),
                                    reference: None,
                                    master: None,
                                    settings,
                                };
                            }
                            Change::Reference(reference) => {
                                if reference.is_none() {
                                    store.session.settings.match_loudness = false;
                                }
                                store.session.settings.reference =
                                    reference.as_ref().map(|r| r.source.path.clone());
                                store.session.reference = reference;
                                store.session.master = None;
                            }
                            Change::Master(master) => {
                                store.session.settings = master.settings.clone();
                                store.session.master = Some(master);
                            }
                        }
                    }
                    ("completed", Some(product.result), None)
                }
                _ if late_cancel => ("cancelled", None, None),
                Err(error) => (
                    "failed",
                    None,
                    Some(engine.policy.redact(&format!("{error:#}"))),
                ),
                Ok(_) => unreachable!(),
            };
            if let Some(job) = store.jobs.get_mut(&job_id) {
                job.status = status.into();
                job.result = result;
                job.error = error;
                job.elapsed_seconds = Some(job.started.elapsed().as_secs_f64());
                job.message = match status {
                    "completed" => "Local processing complete",
                    "cancelled" => "Cancelled; previous session preserved unless cleared",
                    _ => "Processing failed; inspect error",
                }
                .into();
                if status == "completed" {
                    job.progress = 1.;
                }
            }
            store.active = None;
        });
        Ok(
            json!({"job_id":id,"status":"running","next":"Poll get_job_status with this ID; cancel_job requests cancellation."}),
        )
    }

    pub fn job(&self, id: &str) -> Result<Value> {
        let store = self.lock()?;
        let job = store.jobs.get(id).context("Unknown or expired job ID")?;
        let mut value = serde_json::to_value(job)?;
        if job.elapsed_seconds.is_none() {
            value["elapsed_seconds"] = job.started.elapsed().as_secs_f64().into();
        }
        Ok(value)
    }

    pub fn cancel(&self, id: &str) -> Result<Value> {
        let mut store = self.lock()?;
        ensure!(store.jobs.contains_key(id), "Unknown or expired job ID");
        if let Some(active) = &store.active
            && active.id == id
        {
            active.cancel.store(true, Ordering::Relaxed);
            if let Some(job) = store.jobs.get_mut(id) {
                job.status = "cancellation_requested".into();
                job.message =
                    "Waiting for the current DSP checkpoint; original files remain unchanged"
                        .into();
            }
        }
        drop(store);
        self.job(id)
    }
}
