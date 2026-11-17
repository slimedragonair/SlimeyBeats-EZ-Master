//! Metadata-only empty track creation after the audio establishes project layout.
use super::arrangement_state::ArrangementEditor;
use super::arrangement_state::Inspector;
use crate::studio::arrangement::{Edit, Lane};
use anyhow::{Context, Result};

pub(super) fn add_empty(editor: &mut ArrangementEditor) -> Result<()> {
    editor.commit_names()?;
    let count = editor
        .document()
        .context("Add audio first to establish the project layout")?
        .lanes
        .len();
    let id = editor.fresh_id("track");
    editor.apply(Edit::AddLane {
        lane: Lane::new(&id, format!("Track {}", count + 1)),
    })?;
    editor.select_lane(&id)?;
    editor.inspector = Inspector::Track;
    Ok(())
}
