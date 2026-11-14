use crate::state::Engine;
use anyhow::Result;
use mastercraft::audio::Audio;
use std::{io::Write, sync::Arc};
use tempfile::TempPath;

pub fn preview(engine: &Engine, audio: &Audio) -> Result<Arc<TempPath>> {
    engine.policy.check_output()?;
    let mut file = tempfile::Builder::new()
        .prefix("studio-")
        .suffix(".wav")
        .tempfile_in(engine.cache.path())?;
    mastercraft::wav_encoding::write_wav(audio, file.as_file_mut(), "wav32f", false)?;
    file.flush()?;
    file.as_file().sync_all()?;
    Ok(Arc::new(file.into_temp_path()))
}

pub fn relative_exports(
    engine: &Engine,
    mut exports: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>> {
    use anyhow::Context;
    for item in &mut exports {
        let path = item["path"].as_str().context("Exporter returned no path")?;
        item["path"] = engine
            .policy
            .relative_output(std::path::Path::new(path))?
            .into();
    }
    Ok(exports)
}
pub fn report(engine: &Engine, stem: &str, value: &impl serde::Serialize) -> Result<String> {
    let mut file = tempfile::NamedTempFile::new_in(&engine.policy.output)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.flush()?;
    file.as_file().sync_all()?;
    let path = mastercraft::export::persist_unique(
        file,
        &engine.policy.output,
        &format!("{stem}_report"),
        "json",
    )?;
    engine.policy.relative_output(&path)
}
