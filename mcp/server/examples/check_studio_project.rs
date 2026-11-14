//! Regression helper: use the actual native loader, including launched handoffs.
use anyhow::{Context, Result, ensure};
use mastercraft::studio_native::session::StudioDocument;
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next().context("Expected a Studio project path")?;
    let handoff = first == "--studio-session";
    let path = PathBuf::from(if handoff {
        args.next().context("Expected handoff project path")?
    } else {
        first
    });
    ensure!(args.next().is_none(), "Unexpected argument");
    let loaded = StudioDocument::load(&path)?;
    let direct_stem = loaded.stems.as_ref().and_then(|bank| {
        bank.tracks
            .iter()
            .find(|stem| Arc::ptr_eq(&stem.audio, &loaded.before))
            .map(|stem| stem.kind)
    });
    if let (Some(bank), Some(kind)) = (&loaded.stems, direct_stem) {
        let applied = bank.replace(kind, loaded.preview.clone())?;
        for stem in &bank.tracks {
            let expected = if stem.kind == kind {
                &loaded.preview
            } else {
                &stem.audio
            };
            ensure!(applied.track(stem.kind)?.audio.samples == expected.samples);
        }
    }
    let report = json!({
        "source_name": loaded.source_name,
        "sample_rate": loaded.source.sample_rate,
        "channels": loaded.source.channels,
        "frames": loaded.source.frames(),
        "stems": loaded.stems.as_ref().map_or(0, |bank| bank.tracks.len()),
        "direct_stem": direct_stem,
        "direct_stem_apply_verified": direct_stem.is_some(),
    });
    let bytes = serde_json::to_vec_pretty(&report)?;
    if handoff {
        std::fs::write(path.with_extension("native-reopen.json"), &bytes)?;
    }
    println!("{}", String::from_utf8(bytes)?);
    Ok(())
}
