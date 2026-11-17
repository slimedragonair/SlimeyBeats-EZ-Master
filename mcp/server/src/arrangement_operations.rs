//! Explicit arrangement audio jobs; edits do not call these render paths.
use crate::{
    arrangement_arguments::*,
    arrangement_state::RenderedArrangement,
    artifact_files,
    jobs::{Change, ContextData, Product},
};
use anyhow::Result;
use mastercraft::studio::{arrangement, arrangement_session::ArrangementSession};
use serde_json::json;
use std::{path::Path, sync::Arc};

pub fn suggest(
    ctx: ContextData,
    session: Arc<ArrangementSession>,
    request: SuggestArrangementEq,
) -> Result<Product> {
    ctx.check()?;
    let profile = ctx.engine.profiles.resolve(&request.profile)?;
    let suggestion = session.suggest_eq_replacing(
        &request.target,
        request.exclude_eq_id.as_deref(),
        &profile,
        &|| ctx.cancelled(),
    )?;
    ctx.check()?;
    let mut value = serde_json::to_value(suggestion)?;
    value["session_id"] = session.epoch().into();
    value["revision"] = session.revision().into();
    Ok(Product::value(value))
}
pub fn render(ctx: ContextData, session: Arc<ArrangementSession>) -> Result<Product> {
    ctx.progress(
        0.05,
        "Rendering the explicitly requested arrangement preview",
    );
    let audio = arrangement::render(
        session.document(),
        session.assets(),
        session.render_options(),
        &|| ctx.cancelled(),
    )?;
    ctx.check()?;
    let file = artifact_files::preview(&ctx.engine, &audio)?;
    let preview = Arc::new(RenderedArrangement {
        id: format!("arrangement-{}", ctx.id),
        epoch: session.epoch().into(),
        revision: session.revision(),
        file,
        frames: audio.frames() as u64,
        sample_rate: audio.sample_rate,
        channels: audio.channels,
    });
    ctx.check()?;
    let result = json!({"session_id":preview.epoch,"revision":preview.revision,"preview_id":preview.id,
        "relative_preview_path":ctx.engine.policy.relative_output(preview.file.as_ref())?,
        "frames":preview.frames,"sample_rate":preview.sample_rate,"channels":preview.channels,
        "next":"Temporary rendered preview only. Listen and inspect; metadata edits require another explicit render. Export is separate."});
    Ok(Product {
        result,
        change: Change::ArrangementPreview(preview),
        committed: false,
    })
}
pub fn export(
    ctx: ContextData,
    session: Arc<ArrangementSession>,
    request: ExportArrangement,
) -> Result<Product> {
    crate::file_policy::export_stem(&request.name)?;
    ctx.engine.policy.check_output()?;
    ctx.progress(
        0.05,
        "Rendering the explicit arrangement export through shared scoped effects",
    );
    let audio = arrangement::render(
        session.document(),
        session.assets(),
        session.render_options(),
        &|| ctx.cancelled(),
    )?;
    let settings = mastercraft::config::Settings {
        out_dir: ctx.engine.policy.output.clone(),
        formats: vec![request.format.name().into()],
        output_sr: request.sample_rate,
        dither: request.dither,
        ceiling_dbtp: session.render_options().ceiling_dbtp,
        suffix: String::new(),
        ..Default::default()
    };
    settings.validate()?;
    let exports = mastercraft::export::export(
        &audio,
        Path::new(&format!("{}.wav", request.name)),
        &settings,
        &|| ctx.cancelled(),
    )?;
    let exports = artifact_files::relative_exports(&ctx.engine, exports)?;
    Ok(Product {
        result: json!({"session_id":session.epoch(),"revision":session.revision(),"exports":exports}),
        change: Change::None,
        committed: true,
    })
}
