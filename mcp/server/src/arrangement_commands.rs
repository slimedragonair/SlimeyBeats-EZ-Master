//! Metadata transactions and queries. Audio work belongs to explicit jobs.
use crate::{arrangement_arguments::*, state::Engine};
use anyhow::{Context, Result, ensure};
use mastercraft::studio::{
    arrangement::{Clip, Edit, Lane},
    arrangement_session::{ArrangementSession, OwnedAsset, SessionLimits, edit_label},
};
use serde_json::Value;
use std::sync::Arc;

pub fn create(engine: &Engine, request: CreateArrangement) -> Result<Value> {
    let mut store = engine.lock()?;
    ensure!(
        store.active.is_none(),
        "Wait for the active audio job first"
    );
    if let Some(old) = &store.arrangements.current {
        ensure!(
            request.replace_session_id.as_deref() == Some(old.epoch()),
            "Replacing the workspace requires its current replace_session_id"
        );
    }
    store.sequence += 1;
    let id = format!("arr-{}-{}", engine.connection_id, store.sequence);
    let session = ArrangementSession::new(
        request.sample_rate,
        request.channels,
        id,
        SessionLimits {
            max_decoded_bytes: (engine.max_decoded_bytes as u64)
                .saturating_mul(8)
                .min(4 * 1024 * 1024 * 1024),
            ..Default::default()
        },
    )?;
    store.arrangements.current = Some(Arc::new(session));
    store.arrangements.preview = None;
    store.arrangements.summary(engine)
}
pub fn read(engine: &Engine) -> Result<Value> {
    engine.lock()?.arrangements.summary(engine)
}
pub fn add(engine: &Engine, request: AddArrangementAssets) -> Result<Value> {
    let mut store = engine.lock()?;
    ensure!(
        store.active.is_none(),
        "Wait for the active audio job first"
    );
    store
        .arrangements
        .selected(&request.session_id, request.expected_revision)?;
    ensure!(
        !request.sources.is_empty(),
        "Select at least one owned audio ID"
    );
    let mut assets = Vec::new();
    let mut edits = Vec::new();
    store.sequence += 1;
    for (index, source) in request.sources.iter().enumerate() {
        let target = store.session.studio_audio(&source.audio_id)?;
        ensure!(
            target.audio.samples.len() * 4 <= engine.max_decoded_bytes,
            "Input exceeds the configured decoded budget"
        );
        let id = format!("arr-{}-{}-{index}", engine.connection_id, store.sequence);
        let name = source.name.clone().unwrap_or(target.label);
        let frames = target.audio.frames() as u64;
        assets.push(OwnedAsset {
            id: id.clone(),
            name: name.clone(),
            audio: target.audio,
        });
        let mut lane = Lane::new(format!("{id}-lane"), name);
        lane.clips.push(Clip {
            id: format!("{id}-clip"),
            name: None,
            source_id: id,
            start_frame: 0,
            source_start_frame: 0,
            end_frame: frames,
            gain_db: 0.,
            fade_in_frames: 0,
            fade_out_frames: 0,
        });
        edits.push(Edit::AddLane { lane });
    }
    Arc::make_mut(
        store
            .arrangements
            .current
            .as_mut()
            .context("Create an arrangement first")?,
    )
    .add_assets(request.expected_revision, assets, &edits, "Add tracks")?;
    store.arrangements.summary(engine)
}
pub fn edit(engine: &Engine, request: EditArrangement) -> Result<Value> {
    let mut store = engine.lock()?;
    ensure!(
        store.active.is_none(),
        "Wait for the active audio job first"
    );
    store
        .arrangements
        .selected(&request.session_id, request.expected_revision)?;
    Arc::make_mut(store.arrangements.current.as_mut().unwrap()).edit(
        request.expected_revision,
        &request.edits,
        &edit_label(&request.edits),
    )?;
    store.arrangements.summary(engine)
}
pub fn history(engine: &Engine, request: ArrangementHistoryRequest) -> Result<Value> {
    let mut store = engine.lock()?;
    ensure!(
        store.active.is_none(),
        "Wait for the active audio job first"
    );
    store
        .arrangements
        .selected(&request.session_id, request.expected_revision)?;
    let session = Arc::make_mut(store.arrangements.current.as_mut().unwrap());
    if request.redo {
        session.redo(request.expected_revision)?;
    } else {
        session.undo(request.expected_revision)?;
    }
    store.arrangements.summary(engine)
}
