//! Publish a rendered stem revision and its full mix as one atomic Studio change.
use crate::{
    jobs::{Change, ContextData, Product},
    state::Snapshot,
    studio_operations,
    studio_state::{AudioTarget, StemBank, StudioState, stem_id},
};
use anyhow::Result;
use mastercraft::studio::{
    region_edit::{self, RegionEdit, RegionOperation},
    stem_mix::{self, StemKind},
};
use serde_json::{Value, json};
use std::sync::Arc;

pub(crate) fn validate(
    snapshot: &Snapshot,
    bank_id: &str,
    kind: StemKind,
    edit: &RegionEdit,
) -> Result<()> {
    let bank = snapshot.studio_bank(bank_id)?;
    edit.validate(&bank.stems.track(kind)?.audio)
}

// An isolated, unapplied stem preview is not the bank's committed mix lineage.
fn mix_steps(studio: &StudioState, bank: &Arc<StemBank>) -> Vec<Value> {
    studio
        .current
        .iter()
        .chain(studio.undo.iter().rev())
        .find(|preview| {
            preview
                .bank_snapshot
                .as_ref()
                .is_some_and(|saved| Arc::ptr_eq(saved, bank))
                && matches!(
                    preview
                        .steps
                        .last()
                        .and_then(|step| step["operation"].as_str()),
                    Some(
                        "import_stems"
                            | "import_stem_bundle"
                            | "separate_stems"
                            | "stem_mix"
                            | "replace_stem"
                            | "edit_stem_region"
                    )
                )
        })
        .map(|preview| preview.steps.clone())
        .unwrap_or_default()
}

pub fn render(
    ctx: ContextData,
    bank_id: String,
    kind: StemKind,
    edit: RegionEdit,
) -> Result<Product> {
    let bank = ctx.snapshot.studio_bank(&bank_id)?;
    ctx.progress(0.05, "Rendering selected stem frames from the current bank");
    let edited = region_edit::render(&bank.stems.track(kind)?.audio, &edit, &|| ctx.cancelled())?;
    let stems = bank.stems.replace(kind, Arc::new(edited))?;
    ctx.progress(
        0.35,
        "Preparing the comparison mix with current stem controls",
    );
    let before = Arc::new(stem_mix::render(&bank.stems, &bank.controls, &|| {
        ctx.cancelled()
    })?);
    ctx.progress(
        0.6,
        "Rendering the whole mix with the edited stem and current controls",
    );
    let audio = stem_mix::render(&stems, &bank.controls, &|| ctx.cancelled())?;
    let next_bank = Arc::new(StemBank {
        id: format!("stems-{}", ctx.id),
        source_name: bank.source_name.clone(),
        stems,
        controls: bank.controls.clone(),
        model: bank.model.clone(),
        original_mix: bank.original_mix.clone(),
    });
    let mut target = AudioTarget::bank_mix(&next_bank, "Mix with edited stem region");
    target.audio = before;
    target.steps = mix_steps(&ctx.snapshot.studio, &bank);
    let mut studio = ctx.snapshot.studio.clone();
    studio.bank = Some(next_bank.clone());
    let action = match &edit.operation {
        RegionOperation::Gain { db } => format!("Gain ({db} dB)"),
        operation => operation.label().into(),
    };
    let rendered_note = format!(
        "Audio edit rendered: {action} on {} [{}], {} to {} seconds. Length, rate and channels are retained. Raw samples outside the selected frames and all other stems retain their exact bits.",
        bank.stems.display_name(kind),
        kind.name(),
        edit.start_seconds,
        edit.end_seconds,
    );
    let mut product = studio_operations::finish(
        &ctx,
        target,
        audio,
        json!({"operation":"edit_stem_region","input_audio_id":stem_id(&bank_id,kind),"stem_set_id":bank_id,"kind":kind,"edit":edit}),
        vec![
            rendered_note,
            "This temporary whole-mix preview uses the existing gain, balance, mute, solo and output/peak controls. Desktop faders remain live; mix peak protection can affect the rendered mix beyond the edited selection. Audition before accepting; no final audio was exported.".into(),
        ],
        studio,
    )?;
    let Change::Studio(prepared) = &product.change else {
        unreachable!("Studio finish always prepares a Studio change")
    };
    let summary = prepared.summary(&ctx.engine, &ctx.snapshot)?;
    product
        .result
        .as_object_mut()
        .expect("Studio result object")
        .extend(summary.as_object().expect("Studio summary object").clone());
    product.result["stem_set_id"] = next_bank.id.clone().into();
    Ok(product)
}

#[cfg(test)]
#[path = "region_edit_operations_tests.rs"]
mod tests;
