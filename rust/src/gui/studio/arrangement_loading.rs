//! Decode/conform and prepare a complete playable import before the UI publishes it.
use super::{
    arrangement_runtime::PreparedSession,
    arrangement_state::{ArrangementEditor, LaneInit},
    arrangement_waveforms::PreparedWaveform,
};
use crate::{
    audio::Audio,
    studio::{
        arrangement::{Arrangement, ArrangementPlayer, RenderOptions},
        arrangement_project::ArrangementSnapshot,
        stem_import_plan::PreferredFormat,
    },
    studio_native::{arrangement_import, audio_import, stem_bundle::PreparedImport},
};
use anyhow::{Context, Result, ensure};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub(super) struct ImportContext {
    pub editor: ArrangementEditor,
    pub expected_epoch: u64,
    pub reference: Option<Arc<Audio>>,
    pub origin: Option<Arrangement>,
    pub options: RenderOptions,
}

pub(super) fn prepare_named(
    context: ImportContext,
    sources: Vec<(String, Arc<Audio>, LaneInit)>,
    cancel: &dyn Fn() -> bool,
    progress: &dyn Fn(f32, &str),
) -> Result<PreparedSession> {
    let ImportContext {
        mut editor,
        expected_epoch,
        reference,
        origin,
        options,
    } = context;
    ensure!(!cancel(), "Cancelled");
    let previous: std::collections::BTreeSet<_> = editor.assets.keys().cloned().collect();
    editor.add_sources_with_controls(sources)?;
    let document = editor
        .document()
        .context("Imported project has no document")?
        .clone();
    let origin = origin.unwrap_or_else(|| document.clone());
    let mut waveforms = Vec::new();
    for (id, audio) in &editor.assets {
        if !previous.contains(id) {
            ensure!(!cancel(), "Cancelled");
            progress(0.72, "Preparing stereo waveform peaks");
            waveforms.push((id.clone(), PreparedWaveform::new(audio.clone())?));
        }
    }
    progress(0.82, "Preparing live multitrack playback");
    let player = ArrangementPlayer::new_cancellable(&document, &editor.assets, &options, cancel)?;
    let reference = if let Some(reference) = reference {
        reference
    } else {
        progress(0.88, "Preparing the initial-import comparison");
        Arc::new(crate::studio::arrangement::render(
            &origin,
            &editor.assets,
            &options,
            cancel,
        )?)
    };
    ensure!(!cancel(), "Cancelled");
    Ok(PreparedSession {
        expected_epoch,
        editor,
        waveforms,
        player,
        reference,
        origin,
        options,
        loaded_project: false,
    })
}

pub(super) fn load_bundle(
    context: ImportContext,
    prepared: PreparedImport,
    selected: Vec<usize>,
    preferred: PreferredFormat,
    working: &Path,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(f32, &str),
) -> Result<PreparedSession> {
    let target = context
        .editor
        .document()
        .map(|document| document.sample_rate);
    let retained = context
        .editor
        .assets
        .values()
        .try_fold(0_u64, |total, audio| {
            total
                .checked_add(audio.samples.len() as u64 * 4)
                .context("Source pool byte count overflow")
        })?;
    let mut remaining = super::arrangement_runtime::NATIVE_SOURCE_BUDGET
        .checked_sub(retained)
        .context("Arrangement source pool is full")?;
    prepared.with_audio_files(&selected, &preferred, cancel.clone(), progress, |files| {
        crate::gui::tool_setup::for_inputs(
            files.iter().map(|(_, path)| path),
            cancel.clone(),
            progress,
        )?;
        let mut rate = target;
        let mut sources = Vec::new();
        for (index, (name, path)) in files.iter().enumerate() {
            ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
            progress(
                0.2 + 0.45 * index as f32 / files.len().max(1) as f32,
                &format!("Loading track {} of {}", index + 1, files.len()),
            );
            let maximum = (crate::studio_native::stem_files::DEFAULT_INPUT_DECODED_BYTES)
                .min(remaining.min(usize::MAX as u64) as usize);
            let (audio, _) =
                audio_import::load(path, maximum, Duration::from_secs(300), cancel.clone())?;
            let project_rate = *rate.get_or_insert(audio.sample_rate);
            if audio.sample_rate != project_rate {
                crate::gui::tool_setup::prepare(true, cancel.clone(), progress)?;
            }
            let audio =
                arrangement_import::conform(audio, project_rate, maximum, working, cancel.clone())?;
            remaining = remaining
                .checked_sub(audio.samples.len() as u64 * 4)
                .context("Arrangement source pool is full")?;
            sources.push((name.clone(), Arc::new(audio), LaneInit::default()));
        }
        prepare_named(
            context,
            sources,
            &|| cancel.load(Ordering::Relaxed),
            progress,
        )
    })
}

pub(super) fn prepare_project(
    snapshot: ArrangementSnapshot,
    expected_epoch: u64,
    cancel: &dyn Fn() -> bool,
    progress: &dyn Fn(f32, &str),
) -> Result<PreparedSession> {
    ensure!(!cancel(), "Cancelled");
    snapshot.validate()?;
    let origin = snapshot
        .origin
        .clone()
        .unwrap_or_else(|| snapshot.document().clone());
    let options = snapshot.render_options.clone();
    let mut editor = ArrangementEditor::restore(
        snapshot.history,
        snapshot.assets,
        snapshot.asset_names,
        expected_epoch + 1,
    )?;
    editor.fit();
    if let Some(id) = editor
        .document()
        .and_then(|document| document.lanes.first())
        .and_then(|lane| lane.clips.first())
        .map(|clip| clip.id.clone())
    {
        editor.select_clip(&id)?;
    }
    editor.refresh_eq();
    let mut waveforms = Vec::new();
    for (id, audio) in &editor.assets {
        ensure!(!cancel(), "Cancelled");
        progress(0.5, "Preparing project waveform peaks");
        waveforms.push((id.clone(), PreparedWaveform::new(audio.clone())?));
    }
    let player = ArrangementPlayer::new_cancellable(
        editor.document().context("Missing project document")?,
        &editor.assets,
        &options,
        cancel,
    )?;
    let reference = Arc::new(crate::studio::arrangement::render(
        &origin,
        &editor.assets,
        &options,
        cancel,
    )?);
    ensure!(!cancel(), "Cancelled");
    Ok(PreparedSession {
        expected_epoch,
        editor,
        waveforms,
        player,
        reference,
        origin,
        options,
        loaded_project: true,
    })
}
