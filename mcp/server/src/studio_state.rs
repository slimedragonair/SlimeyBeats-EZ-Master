//! A separate, bounded editing workspace; it does not silently replace a master.
use crate::state::{Engine, Snapshot};
use anyhow::{Context, Result, ensure};
use mastercraft::{
    audio::Audio,
    studio::stem_mix::{MixSettings, StemKind, StemSet},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tempfile::TempPath;

pub struct EditedAudio {
    pub id: String,
    pub label: String,
    pub audio: Arc<Audio>,
    pub before: Arc<Audio>,
    pub file: Option<Arc<TempPath>>,
    pub steps: Vec<Value>,
    pub notes: Vec<String>,
    pub bank_snapshot: Option<Arc<StemBank>>,
}
pub struct StemBank {
    pub id: String,
    pub source_name: String,
    pub stems: StemSet,
    pub controls: MixSettings,
    pub model: Value,
    pub original_mix: Arc<Audio>,
}
#[derive(Clone)]
pub struct StudioOrigin {
    pub id: String,
    pub label: String,
    pub audio: Arc<Audio>,
}

#[derive(Clone, Default)]
pub struct StudioState {
    pub origin: Option<StudioOrigin>,
    pub bank: Option<Arc<StemBank>>,
    pub current: Option<Arc<EditedAudio>>,
    pub undo: Vec<Arc<EditedAudio>>,
    pub redo: Vec<Arc<EditedAudio>>,
}
pub struct AudioTarget {
    pub audio: Arc<Audio>,
    pub label: String,
    pub steps: Vec<Value>,
}

impl Snapshot {
    pub fn studio_audio(&self, id: &str) -> Result<AudioTarget> {
        if let Some(source) = &self.source
            && source.id == id
        {
            return Ok(AudioTarget {
                audio: source.audio.clone(),
                label: source
                    .source
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                steps: vec![],
            });
        }
        if let Some(origin) = &self.studio.origin
            && origin.id == id
        {
            return Ok(AudioTarget {
                audio: origin.audio.clone(),
                label: origin.label.clone(),
                steps: vec![],
            });
        }
        if let Some(preview) = &self.studio.current
            && preview.id == id
        {
            return Ok(AudioTarget {
                audio: preview.audio.clone(),
                label: preview.label.clone(),
                steps: preview.steps.clone(),
            });
        }
        if let Some(bank) = &self.studio.bank {
            if id == format!("{}:original_mix", bank.id) {
                return Ok(AudioTarget {
                    audio: bank.original_mix.clone(),
                    label: "Stem source mix".into(),
                    steps: vec![],
                });
            }
            for stem in &bank.stems.tracks {
                if id == stem_id(&bank.id, stem.kind) {
                    return Ok(AudioTarget {
                        audio: stem.audio.clone(),
                        label: format!("{} stem", stem.kind.name()),
                        steps: vec![
                            json!({"operation":"stem_source","kind":stem.kind,"model":bank.model}),
                        ],
                    });
                }
            }
        }
        anyhow::bail!(
            "Unknown/stale audio ID; call get_studio_session for currently available source, stem and preview IDs"
        )
    }
    pub fn studio_bank(&self, id: &str) -> Result<Arc<StemBank>> {
        let bank = self
            .studio
            .bank
            .as_ref()
            .context("Separate or import aligned stems first")?;
        ensure!(
            bank.id == id,
            "Stem-set ID is stale; call get_studio_session"
        );
        Ok(bank.clone())
    }
}

impl StudioState {
    pub fn push(&mut self, preview: Arc<EditedAudio>, budget: usize) {
        if let Some(current) = self.current.take() {
            self.undo.push(current);
        }
        self.current = Some(preview);
        self.redo.clear();
        while self.undo.len() > 4
            || self
                .undo
                .iter()
                .map(|p| p.audio.samples.len() * 4)
                .sum::<usize>()
                > budget
        {
            if self.undo.is_empty() {
                break;
            }
            self.undo.remove(0);
        }
    }
    pub fn summary(&self, engine: &Engine, snapshot: &Snapshot) -> Result<Value> {
        let mut assets = Vec::new();
        if let Some(source) = &snapshot.source {
            assets.push(audio_info(&source.id, "Loaded source", &source.audio));
        }
        if let Some(origin) = &self.origin
            && snapshot
                .source
                .as_ref()
                .is_none_or(|source| source.id != origin.id)
        {
            assets.push(audio_info(&origin.id, "Studio original", &origin.audio));
        }
        if let Some(bank) = &self.bank {
            assets.push(audio_info(
                &format!("{}:original_mix", bank.id),
                "Stem source mix",
                &bank.original_mix,
            ));
            for stem in &bank.stems.tracks {
                assets.push(audio_info(
                    &stem_id(&bank.id, stem.kind),
                    stem.kind.name(),
                    &stem.audio,
                ));
            }
        }
        if let Some(preview) = &self.current {
            assets.push(audio_info(&preview.id, &preview.label, &preview.audio));
        }
        let preview=self.current.as_ref().map(|p|Ok::<_,anyhow::Error>(json!({"studio_preview_id":p.id,"label":p.label,"relative_preview_path":p.file.as_ref().map(|file|engine.policy.relative_output(file.as_ref())).transpose()?,"steps":p.steps,"notes":p.notes}))).transpose()?;
        Ok(
            json!({"assets":assets,"stem_set":self.bank.as_ref().map(|b|json!({"stem_set_id":b.id,"controls":b.controls,"model":b.model})),"preview":preview,"can_undo":!self.undo.is_empty(),"can_redo":!self.redo.is_empty(),"note":"Studio revisions are independent of the mastering preview. Export explicitly, or promote a prepared mix to mastering when ready."}),
        )
    }
}
pub fn stem_id(bank: &str, kind: StemKind) -> String {
    format!("{bank}:{}", kind.name())
}
fn audio_info(id: &str, label: &str, audio: &Audio) -> Value {
    json!({"audio_id":id,"label":label,"duration_seconds":audio.duration(),"sample_rate":audio.sample_rate,"channels":audio.channels})
}
