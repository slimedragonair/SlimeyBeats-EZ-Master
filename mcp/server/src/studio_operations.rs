use crate::{
    artifact_files,
    jobs::{Change, ContextData, Product},
    studio_state::{AudioTarget, EditedAudio, StemBank, StudioState},
};
use anyhow::{Result, ensure};
use mastercraft::{
    audio::Audio,
    studio::{
        spectral_repair::{self, RepairSelection},
        stem_mix::{self, MixSettings},
        voice_cleanup::{self, VoiceSettings},
    },
    studio_native::{inference::SeparationMode, runtime::StemRuntime},
};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn finish(
    ctx: &ContextData,
    target: AudioTarget,
    audio: Audio,
    step: Value,
    notes: Vec<String>,
    mut studio: StudioState,
) -> Result<Product> {
    ctx.check()?;
    let file = artifact_files::preview(&ctx.engine, &audio)?;
    ctx.check()?;
    if studio.current.is_none() {
        studio.current = Some(Arc::new(EditedAudio {
            id: step["input_audio_id"].as_str().unwrap_or("original").into(),
            label: target.label.clone(),
            audio: target.audio.clone(),
            file: None,
            steps: target.steps.clone(),
            notes: vec![],
            bank_snapshot: studio.bank.clone(),
        }));
    }
    let mut steps = target.steps;
    steps.push(step);
    let id = format!("studio-{}", ctx.id);
    let measurements = mastercraft::meters::measure(&audio)?;
    let result = json!({"studio_preview_id":id,"relative_preview_path":ctx.engine.policy.relative_output(file.as_ref())?,"measurements":measurements,"notes":notes,"next":"Preview is temporary. Inspect/listen, adjust or undo, then export explicitly. Studio edits do not silently alter the mastering preview."});
    let edited = Arc::new(EditedAudio {
        id,
        label: target.label,
        audio: Arc::new(audio),
        file: Some(file),
        steps,
        notes,
        bank_snapshot: studio.bank.clone(),
    });
    studio.push(edited, ctx.engine.max_decoded_bytes.saturating_mul(2));
    Ok(Product {
        result,
        change: Change::Studio(studio),
        committed: false,
    })
}

pub fn voice(ctx: ContextData, audio_id: String, settings: VoiceSettings) -> Result<Product> {
    let target = ctx.snapshot.studio_audio(&audio_id)?;
    ctx.progress(
        0.1,
        "Preparing voice cleanup from the selected immutable source",
    );
    let cleaned = voice_cleanup::render(&target.audio, &settings, &|| ctx.cancelled())?;
    finish(
        &ctx,
        target,
        cleaned.audio,
        json!({"operation":"voice_cleanup","input_audio_id":audio_id,"settings":settings}),
        cleaned.notes,
        ctx.snapshot.studio.clone(),
    )
}

pub fn repair(ctx: ContextData, audio_id: String, selection: RepairSelection) -> Result<Product> {
    let target = ctx.snapshot.studio_audio(&audio_id)?;
    ctx.progress(0.1, "Repairing only the selected time/frequency region");
    let audio = spectral_repair::render(&target.audio, &selection, &|| ctx.cancelled())?;
    finish(&ctx,target,audio,json!({"operation":"spectral_repair","input_audio_id":audio_id,"selection":selection}),vec!["Samples outside the selected time region were retained. Healing estimates magnitudes from surrounding audio; audition before accepting.".into()],ctx.snapshot.studio.clone())
}

pub fn separate(ctx: ContextData, source_id: String, mode: SeparationMode) -> Result<Product> {
    let source = ctx.snapshot.source(&source_id)?;
    ensure!(
        ctx.engine.stem_runtime.is_ready(),
        "Local stem engine is not installed. Read get_help(stems) and install_stem_engine when the user requests its download/setup"
    );
    let runtime = StemRuntime::from(&ctx.engine.stem_runtime);
    let separated = runtime.separate(
        &source.audio,
        mode,
        ctx.engine.cache.path(),
        &|fraction, message| ctx.progress(fraction * 0.9, message),
        ctx.cancel.clone(),
    )?;
    let controls = separated.stems.defaults();
    let audio = stem_mix::render(&separated.stems, &controls, &|| ctx.cancelled())?;
    let bank = Arc::new(StemBank {
        id: format!("stems-{}", ctx.id),
        stems: separated.stems,
        controls,
        model: separated.metadata,
        original_mix: source.audio.clone(),
    });
    let mut studio = StudioState {
        bank: Some(bank.clone()),
        ..Default::default()
    };
    let id = format!("studio-{}", ctx.id);
    let file = artifact_files::preview(&ctx.engine, &audio)?;
    studio.current=Some(Arc::new(EditedAudio {id:id.clone(),label:"Stem mix".into(),audio:Arc::new(audio),file:Some(file),steps:vec![json!({"operation":"separate_stems","source_id":source_id,"source_sha256":source.source.identity.sha256,"mode":mode})],notes:vec!["Separation is an estimate, not perfect isolation. Reconstruction residual is retained in instrumental/other so the neutral mix preserves the source.".into()],bank_snapshot:Some(bank.clone())}));
    let result = studio.summary(&ctx.engine, &ctx.snapshot)?;
    ctx.check()?;
    Ok(Product {
        result,
        change: Change::Studio(studio),
        committed: false,
    })
}

pub fn mix(ctx: ContextData, bank_id: String, settings: MixSettings) -> Result<Product> {
    let bank = ctx.snapshot.studio_bank(&bank_id)?;
    let audio = stem_mix::render(&bank.stems, &settings, &|| ctx.cancelled())?;
    let mut studio = ctx.snapshot.studio.clone();
    studio.bank = Some(Arc::new(StemBank {
        id: bank.id.clone(),
        stems: bank.stems.clone(),
        controls: settings.clone(),
        model: bank.model.clone(),
        original_mix: bank.original_mix.clone(),
    }));
    let target = AudioTarget {
        audio: bank.original_mix.clone(),
        label: "Stem mix".into(),
        steps: vec![],
    };
    finish(
        &ctx,
        target,
        audio,
        json!({"operation":"stem_mix","input_audio_id":bank_id,"settings":settings}),
        vec![
            "Rendered from the immutable stem bank; prior fader renders do not accumulate.".into(),
        ],
        studio,
    )
}

pub fn history(ctx: ContextData, redo: bool) -> Result<Product> {
    let mut studio = ctx.snapshot.studio.clone();
    let next = if redo {
        studio.redo.pop()
    } else {
        studio.undo.pop()
    }
    .ok_or_else(|| {
        anyhow::anyhow!(
            "No {} step is available",
            if redo { "redo" } else { "undo" }
        )
    })?;
    if let Some(current) = studio.current.replace(next.clone()) {
        if redo {
            studio.undo.push(current);
        } else {
            studio.redo.push(current);
        }
    }
    studio.bank = next.bank_snapshot.clone();
    let result = studio.summary(&ctx.engine, &ctx.snapshot)?;
    Ok(Product {
        result,
        change: Change::Studio(studio),
        committed: false,
    })
}
