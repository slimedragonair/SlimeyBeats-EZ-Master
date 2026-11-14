use crate::{studio::stem_mix::StemSet, wav_encoding};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub fn import(
    files: &[(crate::studio::stem_mix::StemKind, std::path::PathBuf)],
    cancel: std::sync::Arc<AtomicBool>,
    progress: &dyn Fn(f32, &str),
) -> Result<StemSet> {
    import_with_limits(
        files,
        512 * 1024 * 1024,
        std::time::Duration::from_secs(300),
        cancel,
        progress,
    )
}

pub fn import_with_limits(
    files: &[(crate::studio::stem_mix::StemKind, std::path::PathBuf)],
    max_decoded_bytes: usize,
    timeout: std::time::Duration,
    cancel: std::sync::Arc<AtomicBool>,
    progress: &dyn Fn(f32, &str),
) -> Result<StemSet> {
    crate::studio::stem_mix::validate_kinds(files.iter().map(|(kind, _)| *kind))?;
    let mut tracks = Vec::new();
    for (index, (kind, path)) in files.iter().enumerate() {
        progress(
            index as f32 / files.len() as f32,
            &format!("Loading {} stem", kind.name()),
        );
        let (audio, _) =
            super::audio_import::load(path, max_decoded_bytes, timeout, cancel.clone())?;
        tracks.push(crate::studio::stem_mix::Stem {
            kind: *kind,
            audio: std::sync::Arc::new(audio),
        });
    }
    let mut bank = StemSet::new(tracks)?;
    for (kind, path) in files {
        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .context("Stem filename must contain a UTF-8 name")?;
        bank.set_display_name(*kind, name)?;
    }
    Ok(bank)
}

/// Publish an aligned raw bank as float WAVs. No per-stem normalization can
/// change relative levels; floating-point files retain any original headroom.
pub fn export(stems: &StemSet, destination: &Path, cancel: &AtomicBool) -> Result<Value> {
    std::fs::create_dir_all(destination)?;
    let stage = tempfile::Builder::new()
        .prefix("Studio-stems-")
        .tempdir_in(destination)?;
    let mut files = Vec::new();
    for stem in &stems.tracks {
        ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
        let name = format!("{}.wav", stem.kind.name());
        let file = std::fs::File::create(stage.path().join(&name))?;
        wav_encoding::write_wav(&stem.audio, &file, "wav32f", false)?;
        file.sync_all()?;
        files.push(json!({"file":name,"kind":stem.kind,"display_name":stems.display_name(stem.kind),"frames":stem.audio.frames(),"sample_rate":stem.audio.sample_rate,"channels":stem.audio.channels}));
    }
    let report = json!({"format":"wav32f","stems":files,"notes":"Aligned immutable stem audio, no individual normalization or integer clipping. Float peaks may exceed 0 dBFS; adjust the mix for delivery."});
    let mut manifest = std::fs::File::create(stage.path().join("stems.json"))?;
    serde_json::to_writer_pretty(&mut manifest, &report)?;
    manifest.flush()?;
    manifest.sync_all()?;
    drop(manifest);
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    let folder = stage.keep();
    Ok(json!({"folder":folder,"manifest":report}))
}
