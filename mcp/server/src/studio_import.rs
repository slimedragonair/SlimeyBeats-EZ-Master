//! Shared commit path for loose-file and ZIP imports; processing is still a preview.
use crate::{
    jobs::{Change, ContextData, Product},
    studio_state::{EditedAudio, StemBank, StudioState},
};
use anyhow::Result;
use mastercraft::studio::stem_mix::{self, StemSet};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn prepared(ctx: &ContextData, stems: StemSet, step: Value) -> Result<Product> {
    let controls = stems.defaults();
    let mut raw = controls.clone();
    raw.protect_peaks = false;
    let original_mix = Arc::new(stem_mix::render(&stems, &raw, &|| ctx.cancelled())?);
    let audio = stem_mix::render(&stems, &controls, &|| ctx.cancelled())?;
    let bank = Arc::new(StemBank {
        id: format!("stems-{}", ctx.id),
        source_name: "Imported stem mix.wav".into(),
        stems,
        controls,
        model: json!({"origin":"imported aligned stems"}),
        original_mix,
    });
    let file = crate::artifact_files::preview(&ctx.engine, &audio)?;
    let mut studio = StudioState {
        bank: Some(bank.clone()),
        ..Default::default()
    };
    studio.current = Some(Arc::new(EditedAudio {
        id: format!("studio-{}", ctx.id),
        label: "Imported stem mix".into(),
        audio: Arc::new(audio),
        before: bank.original_mix.clone(),
        file: Some(file),
        steps: vec![step],
        notes: vec![],
        bank_snapshot: Some(bank),
    }));
    let result = studio.summary(&ctx.engine, &ctx.snapshot)?;
    ctx.check()?;
    Ok(Product {
        result,
        change: Change::Studio(studio),
        committed: false,
    })
}
