use crate::{studio::stem_mix::StemSet, wav_encoding};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

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
        files.push(json!({"file":name,"kind":stem.kind,"frames":stem.audio.frames(),"sample_rate":stem.audio.sample_rate,"channels":stem.audio.channels}));
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
