//! Resolve immutable origin and bank association from the selected audio, not layout.
use crate::{
    state::Snapshot,
    studio_state::{AudioTarget, EditedAudio, StemBank, StudioOrigin, stem_id},
};
use anyhow::Result;
use serde_json::json;
use std::sync::Arc;

impl AudioTarget {
    pub(crate) fn bank_mix(bank: &Arc<StemBank>, label: &str) -> Self {
        Self {
            audio: bank.original_mix.clone(),
            label: label.into(),
            steps: vec![],
            origin: StudioOrigin {
                id: format!("{}:original_mix", bank.id),
                label: bank.source_name.clone(),
                audio: bank.original_mix.clone(),
            },
            bank_snapshot: Some(bank.clone()),
        }
    }

    fn preview(preview: &EditedAudio) -> Self {
        Self {
            audio: preview.audio.clone(),
            label: preview.label.clone(),
            steps: preview.steps.clone(),
            origin: preview.origin_snapshot.clone(),
            bank_snapshot: preview.bank_snapshot.clone(),
        }
    }
}

impl Snapshot {
    pub fn studio_audio(&self, id: &str) -> Result<AudioTarget> {
        if let Some(source) = &self.source
            && source.id == id
        {
            if let Some(bank) = &self.studio.bank
                && Arc::ptr_eq(&source.audio, &bank.original_mix)
            {
                return Ok(AudioTarget::bank_mix(bank, &bank.source_name));
            }
            // Promotion keeps its own lineage even after another bank or preview
            // replaces the workspace, or the promoted preview leaves undo history.
            if let Some(preview) = &source.studio_preview {
                return Ok(AudioTarget::preview(preview));
            }
            let label = source
                .source
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            return Ok(AudioTarget {
                audio: source.audio.clone(),
                label: label.clone(),
                steps: vec![],
                origin: StudioOrigin {
                    id: id.into(),
                    label,
                    audio: source.audio.clone(),
                },
                bank_snapshot: None,
            });
        }
        if let Some(origin) = &self.studio.origin
            && origin.id == id
        {
            return Ok(AudioTarget {
                audio: origin.audio.clone(),
                label: origin.label.clone(),
                steps: vec![],
                origin: origin.clone(),
                bank_snapshot: self
                    .studio
                    .current
                    .as_ref()
                    .and_then(|preview| preview.bank_snapshot.clone()),
            });
        }
        if let Some(preview) = &self.studio.current
            && preview.id == id
        {
            return Ok(AudioTarget::preview(preview));
        }
        if let Some(bank) = &self.studio.bank {
            if id == format!("{}:original_mix", bank.id) {
                return Ok(AudioTarget::bank_mix(bank, "Stem source mix"));
            }
            for stem in &bank.stems.tracks {
                if id == stem_id(&bank.id, stem.kind) {
                    let mut target = AudioTarget::bank_mix(bank, "");
                    target.audio = stem.audio.clone();
                    target.label = format!("{} stem", bank.stems.display_name(stem.kind));
                    target.steps = vec![
                        json!({"operation":"stem_source","kind":stem.kind,"model":bank.model}),
                    ];
                    return Ok(target);
                }
            }
        }
        anyhow::bail!(
            "Unknown/stale audio ID; call get_studio_session for currently available source, stem and preview IDs"
        )
    }
}
