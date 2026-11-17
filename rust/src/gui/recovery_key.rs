//! A cheap, throttled editing fingerprint; playback time and transient progress are excluded.
use super::{app::MasterCraftApp, recovery_document::SavedTarget};
use anyhow::Result;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

pub(super) fn key(app: &MasterCraftApp) -> Result<u64> {
    let s = &app.studio;
    let mut hash = DefaultHasher::new();
    let target = match s.target {
        super::studio::state::Target::Source => SavedTarget::Source,
        super::studio::state::Target::Preview => SavedTarget::Preview,
        super::studio::state::Target::Stem(k) => SavedTarget::Stem(k),
    };
    serde_json::to_vec(&(
        (
            &app.settings,
            app.selected,
            app.tab,
            app.selected_band,
            app.volume,
            app.match_levels,
        ),
        (
            &s.mix,
            s.mix_dirty,
            &s.voice,
            &s.repair,
            &s.plugin_path,
            &s.plugin_class,
            &s.plugin_values,
        ),
        (
            target,
            s.two_stems,
            s.volume,
            s.match_levels,
            &s.output_dir,
            &s.export_format,
        ),
    ))?
    .hash(&mut hash);
    for track in &app.tracks {
        track.path.hash(&mut hash);
        track
            .result
            .as_ref()
            .map(|r| Arc::as_ptr(r) as usize)
            .hash(&mut hash);
        track
            .recovered_source
            .as_ref()
            .map(|r| Arc::as_ptr(r) as usize)
            .hash(&mut hash);
    }
    if let Some(source) = &s.source {
        source.path.hash(&mut hash);
        (Arc::as_ptr(&source.audio) as usize).hash(&mut hash);
    }
    s.bank
        .as_ref()
        .map(|b| Arc::as_ptr(b) as usize)
        .hash(&mut hash);
    if let Some(r) = &s.current {
        (Arc::as_ptr(&r.before) as usize).hash(&mut hash);
        (Arc::as_ptr(&r.audio) as usize).hash(&mut hash);
        r.bank
            .as_ref()
            .map(|b| Arc::as_ptr(b) as usize)
            .hash(&mut hash);
        serde_json::to_vec(&(&r.label, &r.notes, &r.mix))?.hash(&mut hash);
    }
    s.hash_recovery_arrangement(&mut hash)?;
    Ok(hash.finish())
}
