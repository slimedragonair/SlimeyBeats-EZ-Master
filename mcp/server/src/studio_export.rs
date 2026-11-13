use crate::{
    artifact_files,
    jobs::{Change, ContextData, Product},
    options::ExportFormat,
};
use anyhow::{Context, Result, ensure};
use serde_json::json;
use std::path::Path;

pub fn audio(
    ctx: ContextData,
    id: String,
    name: String,
    format: ExportFormat,
    sample_rate: u32,
    dither: bool,
) -> Result<Product> {
    let preview = ctx
        .snapshot
        .studio
        .current
        .as_ref()
        .context("Prepare a Studio preview first")?;
    ensure!(
        preview.id == id,
        "Studio preview ID is stale; inspect get_studio_session before exporting"
    );
    crate::file_policy::export_stem(&name)?;
    ctx.engine.policy.check_output()?;
    let mut settings = ctx.snapshot.settings.clone();
    settings.out_dir = ctx.engine.policy.output.clone();
    settings.formats = vec![format.name().into()];
    settings.output_sr = sample_rate;
    settings.dither = dither;
    settings.suffix = String::new();
    settings.validate()?;
    ctx.progress(
        0.05,
        "Encoding and independently re-reading the chosen Studio output",
    );
    let exports = mastercraft::export::export(
        &preview.audio,
        Path::new(&format!("{name}.wav")),
        &settings,
        &|| ctx.cancelled(),
    )?;
    let exports = artifact_files::relative_exports(&ctx.engine, exports)?;
    let report = json!({"kind":"studio_audio","engine_version":mastercraft::branding::ENGINE_VERSION,"label":preview.label,"steps":preview.steps,"notes":preview.notes,"exports":exports});
    let report_path = artifact_files::report(&ctx.engine, &name, &report);
    let (report_path, warning) = match report_path {
        Ok(path) => (Some(path), None),
        Err(error) => (
            None,
            Some(
                ctx.engine
                    .policy
                    .redact(&format!("Audio saved, but its report failed: {error:#}")),
            ),
        ),
    };
    Ok(Product {
        result: json!({"exports":exports,"report_path":report_path,"warning":warning}),
        change: Change::None,
        committed: true,
    })
}
