//! Read-only diagnostics and explicit, undoable repairs share the native/WASM core.
use crate::{
    jobs::{Change, ContextData, Product},
    studio_operations,
};
use anyhow::Result;
use mastercraft::studio::doctor::{self, RepairRequest};
use serde_json::json;

pub fn scan(ctx: ContextData, audio_id: String) -> Result<Product> {
    let target = ctx.snapshot.studio_audio(&audio_id)?;
    ctx.progress(0.05, "Checking the selected audio without changing it");
    let report = doctor::scan(&target.audio, &|| ctx.cancelled())?;
    ctx.check()?;
    Ok(Product {
        result: json!({
            "audio_id":audio_id,
            "label":target.label,
            "report":report,
            "next":"Read the timestamps, evidence and notes. Silence and a pause followed by music can be intentional; low-mid buildup is a tonal hypothesis. No audio, history or export changed. Offer a supported repair only when requested, then audition its temporary preview."
        }),
        change: Change::None,
        committed: false,
    })
}

pub fn preview(ctx: ContextData, audio_id: String, request: RepairRequest) -> Result<Product> {
    let target = ctx.snapshot.studio_audio(&audio_id)?;
    ctx.progress(
        0.05,
        "Preparing only the explicitly selected Stem Doctor repair",
    );
    let repaired = doctor::repair(&target.audio, &request, &|| ctx.cancelled())?;
    let changed_samples = repaired.changed_samples;
    let peak_trim_db = repaired.peak_trim_db;
    let mut notes = repaired.notes;
    notes.push("Samples outside the selected region, duration, rate and channels are retained. This does not remove silence, rearrange music, separate instruments or regenerate missing harmonics.".into());
    let mut product = studio_operations::finish(
        &ctx,
        target,
        repaired.audio,
        json!({"operation":"stem_doctor","input_audio_id":audio_id,"repair":request,"changed_samples":changed_samples,"peak_trim_db":peak_trim_db}),
        notes,
        ctx.snapshot.studio.clone(),
    )?;
    product.result["doctor"] =
        json!({"repair":request,"changed_samples":changed_samples,"peak_trim_db":peak_trim_db});
    product.result["next"] = json!(
        "Temporary Studio preview only. Scan its returned studio_preview_id to review the result, audition and undo if needed. A direct stem preview must be applied with apply_preview_to_stem before it changes the stem bank or whole mix. Export is a separate user request."
    );
    Ok(product)
}

#[cfg(test)]
#[path = "doctor_operations_tests.rs"]
mod tests;
