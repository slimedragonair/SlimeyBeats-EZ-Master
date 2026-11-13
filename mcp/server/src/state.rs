use crate::{
    arguments::Arguments,
    file_policy::FilePolicy,
    jobs::{ActiveJob, JobRecord},
};
use anyhow::{Context, Result, ensure};
use mastercraft::{
    analysis::Analysis, audio::Audio, config::Settings, preview::Preview,
    session_document::SessionSource,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};
use tempfile::{TempDir, TempPath};

pub struct Input {
    pub id: String,
    pub source: SessionSource,
    pub audio: Arc<Audio>,
}
pub struct Reference {
    pub source: SessionSource,
    pub analysis: Analysis,
}
pub struct Master {
    pub id: String,
    pub preview: Preview,
    pub settings: Settings,
    pub file: Arc<TempPath>,
}

#[derive(Clone)]
pub struct Snapshot {
    pub source: Option<Arc<Input>>,
    pub reference: Option<Arc<Reference>>,
    pub master: Option<Arc<Master>>,
    pub settings: Settings,
}
impl Snapshot {
    pub fn source(&self, id: &str) -> Result<Arc<Input>> {
        let source = self
            .source
            .as_ref()
            .context("Import audio with load_audio first")?;
        ensure!(
            source.id == id,
            "Source ID is stale; inspect get_session before processing"
        );
        Ok(source.clone())
    }
    pub fn master(&self, id: &str) -> Result<Arc<Master>> {
        let master = self.master.as_ref().context("Master the track first")?;
        ensure!(
            master.id == id,
            "Preview ID is stale; inspect get_session before exporting or editing"
        );
        Ok(master.clone())
    }
}

pub struct Store {
    pub session: Snapshot,
    pub generation: u64,
    pub sequence: u64,
    pub active: Option<ActiveJob>,
    pub jobs: BTreeMap<String, JobRecord>,
    pub order: VecDeque<String>,
}

pub struct Engine {
    pub policy: FilePolicy,
    pub max_decoded_bytes: usize,
    pub decode_timeout: Duration,
    pub cache: TempDir,
    pub desktop: Option<PathBuf>,
    pub(crate) store: Mutex<Store>,
}

impl Engine {
    pub fn new(args: &Arguments) -> Result<Arc<Self>> {
        let policy = FilePolicy::new(&args.input_root, &args.output_root)?;
        let cache = tempfile::Builder::new()
            .prefix(".slimeybeats-mcp-")
            .tempdir_in(&policy.output)?;
        let desktop = if let Some(path) = &args.desktop_exe {
            ensure!(
                path.is_file(),
                "The configured desktop executable is unavailable"
            );
            Some(path.canonicalize()?)
        } else {
            let parent = std::env::current_exe()?
                .parent()
                .context("No executable directory")?
                .to_path_buf();
            let name = if cfg!(windows) {
                "SlimeyBeats-EZ-Master.exe"
            } else {
                "slimeybeats-ez-master-live"
            };
            let path = parent.join(name);
            path.is_file().then(|| path.canonicalize()).transpose()?
        };
        let settings = Settings {
            out_dir: policy.output.clone(),
            ..Default::default()
        };
        Ok(Arc::new(Self {
            max_decoded_bytes: (args.max_decoded_mib as usize)
                .checked_mul(1024 * 1024)
                .context("Input budget is too large on this platform")?,
            decode_timeout: Duration::from_secs(args.decode_timeout_seconds),
            policy,
            cache,
            desktop,
            store: Mutex::new(Store {
                session: Snapshot {
                    source: None,
                    reference: None,
                    master: None,
                    settings,
                },
                generation: 0,
                sequence: 0,
                active: None,
                jobs: BTreeMap::new(),
                order: VecDeque::new(),
            }),
        }))
    }

    pub(crate) fn lock(&self) -> Result<MutexGuard<'_, Store>> {
        self.store
            .lock()
            .map_err(|_| anyhow::anyhow!("Session state is unavailable; restart the companion"))
    }

    pub fn session(&self) -> Result<Value> {
        let store = self.lock()?;
        let s = &store.session;
        let source = s.source.as_ref().map(|source| json!({"source_id":source.id,"file":self.locator(&source.source.path),"duration_seconds":source.audio.duration(),"sample_rate":source.audio.sample_rate,"channels":source.audio.channels}));
        let master = s.master.as_ref().map(|master| Ok::<_,anyhow::Error>(json!({"preview_id":master.id,"relative_preview_path":self.policy.relative_output(master.file.as_ref())?,"measurements":master.preview.report.output.metrics,"settings":public_settings(&master.settings),"target_lufs":master.preview.report.target_lufs}))).transpose()?;
        Ok(
            json!({"source":source,"reference_loaded":s.reference.is_some(),"master":master,"active_job_id":store.active.as_ref().map(|j| &j.id),"settings":public_settings(&s.settings)}),
        )
    }

    pub fn locator(&self, path: &std::path::Path) -> Option<Value> {
        self.policy
            .inputs
            .iter()
            .enumerate()
            .find_map(|(index, root)| {
                path.strip_prefix(root)
                    .ok()
                    .map(|relative| json!({"library":index,"path":relative.to_string_lossy()}))
            })
    }

    pub fn clear(&self) -> Result<Value> {
        let mut store = self.lock()?;
        if let Some(active) = &store.active {
            active
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        store.generation += 1;
        let settings = Settings {
            out_dir: self.policy.output.clone(),
            ..Default::default()
        };
        store.session = Snapshot {
            source: None,
            reference: None,
            master: None,
            settings,
        };
        Ok(json!({"cleared":true,"active_job_cancellation_requested":store.active.is_some()}))
    }

    pub fn shutdown(&self) {
        if let Ok(store) = self.lock()
            && let Some(active) = &store.active
        {
            active
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            if self.lock().is_ok_and(|store| store.active.is_none()) {
                break;
            }
            if std::time::Instant::now() >= deadline {
                eprintln!(
                    "SlimeyBeats: shutdown grace period expired; an unfinished job may leave a temporary cache. Final exports are preserved."
                );
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

pub fn public_settings(settings: &Settings) -> Value {
    let mut value = serde_json::to_value(settings).expect("validated settings serialize");
    let map = value.as_object_mut().expect("settings object");
    map.remove("out_dir");
    map.remove("reference");
    value
}

pub fn analysis_summary(analysis: &Analysis) -> Value {
    let issues = analysis.issues.iter().map(|issue|json!({"key":issue.key,"severity":issue.severity,"active":issue.severity > 0.,"detail":issue.detail})).collect::<Vec<_>>();
    let deviations = analysis
        .deviation_db
        .iter()
        .zip(&analysis.valid)
        .map(|(&value, &valid)| valid.then_some(value))
        .collect::<Vec<_>>();
    json!({"duration_seconds":analysis.duration_s,"sample_rate":analysis.sample_rate,"channels":analysis.channels,
        "measurements":analysis.metrics,"issues":issues,"band_centers_hz":mastercraft::profiles::BANDS,
        "deviation_db":deviations,"valid_bands":analysis.valid,"suggested_profile":analysis.suggested_profile,
        "interpretation":{"severity":"Heuristic indicator from 0 to 100, not a percentage of damage or a quality score. Higher means a stronger detector flag. Different detectors use different scales; do not add/compare them as a common score. Zero means no positive flag, not a diagnosed problem.","valid_bands":"False means unreliable at this rate or above the detected bandwidth cutoff. Its deviation is null; do not infer a tonal defect there.","deviation_db":"Difference from the selected profile after level alignment, in dB. Positive is more energy than that curve, not automatically too much.","listening":"A provisional measurement-based change still needs listening. Tools do not return audio bytes."},
        "note":"Tonal profiles and suggested corrections are heuristics. Audition the result; these measurements are not a universal quality score."})
}
