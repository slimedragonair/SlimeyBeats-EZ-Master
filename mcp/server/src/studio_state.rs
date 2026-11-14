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
    pub origin_snapshot: StudioOrigin,
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
    pub origin: StudioOrigin,
    pub bank_snapshot: Option<Arc<StemBank>>,
}

impl Snapshot {
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
            && self
                .bank
                .as_ref()
                .is_none_or(|bank| origin.id != format!("{}:original_mix", bank.id))
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
                let mut asset = audio_info(
                    &stem_id(&bank.id, stem.kind),
                    &stem.kind.name(),
                    &stem.audio,
                );
                asset["display_name"] = json!(bank.stems.display_name(stem.kind));
                assets.push(asset);
            }
        }
        if let Some(preview) = &self.current {
            assets.push(audio_info(&preview.id, &preview.label, &preview.audio));
        }
        let preview=self.current.as_ref().map(|p|Ok::<_,anyhow::Error>(json!({"studio_preview_id":p.id,"label":p.label,"relative_preview_path":p.file.as_ref().map(|file|engine.policy.relative_output(file.as_ref())).transpose()?,"steps":p.steps,"notes":p.notes}))).transpose()?;
        Ok(
            json!({"assets":assets,"stem_set":self.bank.as_ref().map(|b|json!({"stem_set_id":b.id,"controls":b.controls,"model":b.model,"stem_names":b.stems.tracks.iter().map(|s|(s.kind.name(),b.stems.display_name(s.kind))).collect::<std::collections::BTreeMap<_,_>>()})),"preview":preview,"can_undo":!self.undo.is_empty(),"can_redo":!self.redo.is_empty(),"note":"Studio revisions are independent of the mastering preview. Export explicitly, or promote a prepared mix to mastering when ready."}),
        )
    }
}
pub fn stem_id(bank: &str, kind: StemKind) -> String {
    format!("{bank}:{}", kind.name())
}
fn audio_info(id: &str, label: &str, audio: &Audio) -> Value {
    json!({"audio_id":id,"label":label,"duration_seconds":audio.duration(),"sample_rate":audio.sample_rate,"channels":audio.channels})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arguments::Arguments;
    use clap::Parser;
    use mastercraft::studio::stem_mix::Stem;

    #[test]
    fn stem_assets_keep_legacy_labels_ids_and_expose_bank_display_names() {
        let root = tempfile::tempdir().unwrap();
        let args = Arguments::parse_from([
            "test",
            "--input-root",
            root.path().to_str().unwrap(),
            "--output-root",
            root.path().join("exports").to_str().unwrap(),
        ]);
        let engine = Engine::new(&args).unwrap();
        let audio = Arc::new(Audio::new(vec![0.01; 800], 8000, 1).unwrap());
        let mut stems = StemSet::new(
            [
                StemKind::Vocals,
                StemKind::Part1,
                StemKind::part(64).unwrap(),
            ]
            .into_iter()
            .map(|kind| Stem {
                kind,
                audio: audio.clone(),
            })
            .collect(),
        )
        .unwrap();
        stems
            .set_display_name(StemKind::Vocals, "Song (Vocals)")
            .unwrap();
        stems
            .set_display_name(StemKind::Part1, "Song (Backing Vocals)")
            .unwrap();
        stems
            .set_display_name(StemKind::part(64).unwrap(), "Song (Extra custom part)")
            .unwrap();
        let bank = Arc::new(StemBank {
            id: "stems-test".into(),
            source_name: "Synthetic source".into(),
            controls: stems.defaults(),
            stems,
            model: json!({}),
            original_mix: audio,
        });
        let mut snapshot = engine.lock().unwrap().session.clone();
        snapshot.studio.bank = Some(bank.clone());
        let summary = snapshot.studio.summary(&engine, &snapshot).unwrap();
        for stem in &bank.stems.tracks {
            let id = stem_id(&bank.id, stem.kind);
            let asset = summary["assets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|asset| asset["audio_id"] == id)
                .unwrap();
            assert_eq!(asset["label"], stem.kind.name().as_ref());
            assert_eq!(
                asset["display_name"],
                bank.stems.display_name(stem.kind).as_ref()
            );
            assert_eq!(
                summary["stem_set"]["stem_names"][stem.kind.name().as_ref()],
                bank.stems.display_name(stem.kind).as_ref()
            );
            assert!(Arc::ptr_eq(
                &snapshot.studio_audio(&id).unwrap().audio,
                &stem.audio
            ));
        }
    }
}
