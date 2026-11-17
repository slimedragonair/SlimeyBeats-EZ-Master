//! A separate metadata workspace. Immutable sources survive source/master changes.
use crate::state::Engine;
use anyhow::{Context, Result, ensure};
use mastercraft::studio::arrangement_session::ArrangementSession;
use serde_json::{Value, json};
use std::sync::Arc;
use tempfile::TempPath;

pub struct RenderedArrangement {
    pub id: String,
    pub epoch: String,
    pub revision: u64,
    pub file: Arc<TempPath>,
    pub frames: u64,
    pub sample_rate: u32,
    pub channels: usize,
}
#[derive(Default)]
pub struct ArrangementWorkspace {
    pub current: Option<Arc<ArrangementSession>>,
    pub preview: Option<Arc<RenderedArrangement>>,
}
impl ArrangementWorkspace {
    pub fn selected(&self, id: &str, revision: u64) -> Result<Arc<ArrangementSession>> {
        let current = self
            .current
            .as_ref()
            .context("Create an arrangement first")?;
        ensure!(
            current.epoch() == id,
            "Arrangement ID is stale; call get_arrangement"
        );
        current.check_revision(revision)?;
        Ok(current.clone())
    }
    pub fn summary(&self, engine: &Engine) -> Result<Value> {
        let Some(current) = &self.current else {
            return Ok(json!({"session_id":null,"arrangement":null}));
        };
        let mut value = serde_json::to_value(current.read())?;
        value["session_id"] = current.epoch().into();
        value["defaults"] = json!({"bands":mastercraft::config::default_eq()});
        value["capabilities"] = json!({"metadata_commands":true,"scoped_eq":true,"suggest_eq":true,"waveforms":true,"render_preview":true,"realtime_playback":false});
        value["preview"] = self.preview.as_ref().map(|preview| -> Result<Value> {
            Ok(json!({"preview_id":preview.id,"revision":preview.revision,"stale":preview.epoch != current.epoch() || preview.revision != current.revision(),
                "relative_preview_path":engine.policy.relative_output(preview.file.as_ref())?,"frames":preview.frames,"sample_rate":preview.sample_rate,"channels":preview.channels}))
        }).transpose()?.unwrap_or(Value::Null);
        Ok(value)
    }
}
