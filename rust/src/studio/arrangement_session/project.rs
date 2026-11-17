//! Explicit persistence bridges; ordinary edits never clone/encode source PCM.
use super::{ArrangementSession, AssetInfo, SessionLimits};
use crate::studio::{arrangement::ArrangementHistory, arrangement_project::ArrangementSnapshot};
use anyhow::{Result, ensure};

impl ArrangementSession {
    /// Private adapters can retain revision identity when preparing an isolated
    /// job worker. This changes no public project manifest fields.
    pub fn from_project_at_revision(
        snapshot: ArrangementSnapshot,
        epoch: String,
        limits: SessionLimits,
        revision: u64,
    ) -> Result<Self> {
        ensure!(
            revision < 9_007_199_254_740_991,
            "Arrangement revision budget exhausted"
        );
        let mut value = Self::from_project(snapshot, epoch, limits)?;
        value.revision = revision;
        Ok(value)
    }
    pub fn project_snapshot(&self) -> Result<ArrangementSnapshot> {
        let snapshot = ArrangementSnapshot {
            history: self.history.snapshot(),
            assets: self.assets.clone(),
            asset_names: self
                .catalog
                .iter()
                .map(|(id, value)| (id.clone(), value.name.clone()))
                .collect(),
            render_options: self.options.clone(),
            origin: Some(self.origin.clone()),
            before: Some(self.before.clone()),
            undo_labels: self.undo_labels.clone(),
            redo_labels: self.redo_labels.clone(),
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
    pub fn from_project(
        snapshot: ArrangementSnapshot,
        epoch: String,
        limits: SessionLimits,
    ) -> Result<Self> {
        snapshot.validate()?;
        ensure!(
            snapshot.decoded_bytes()? <= limits.max_decoded_bytes,
            "Project sources exceed this adapter's decoded budget"
        );
        let mut session = Self::new(
            snapshot.document().sample_rate,
            snapshot.document().channels,
            epoch,
            limits,
        )?;
        session.history = ArrangementHistory::from_snapshot(snapshot.history, &snapshot.assets)?;
        session.catalog = snapshot
            .assets
            .iter()
            .map(|(id, audio)| {
                (
                    id.clone(),
                    AssetInfo {
                        id: id.clone(),
                        name: snapshot
                            .asset_names
                            .get(id)
                            .cloned()
                            .unwrap_or_else(|| id.clone()),
                        frames: audio.frames() as u64,
                        sample_rate: audio.sample_rate,
                        channels: audio.channels,
                    },
                )
            })
            .collect();
        session.assets = snapshot.assets;
        let host_output_budget = session.options.max_output_bytes;
        session.options = snapshot.render_options;
        session.options.max_output_bytes = session.options.max_output_bytes.min(host_output_budget);
        session.origin = snapshot
            .origin
            .unwrap_or_else(|| session.document().clone());
        session.before = snapshot
            .before
            .unwrap_or_else(|| session.document().clone());
        session.undo_labels = snapshot.undo_labels;
        session.redo_labels = snapshot.redo_labels;
        // Older v2 manifests may omit optional human labels, while preserving
        // valid history. Fill these from the restored counts for honest Undo UI.
        session
            .undo_labels
            .resize(session.history.undo_len(), "Edit arrangement".into());
        session
            .redo_labels
            .resize(session.history.redo_len(), "Edit arrangement".into());
        Ok(session)
    }
    pub fn restore_project(
        &mut self,
        expected: u64,
        snapshot: ArrangementSnapshot,
        epoch: String,
    ) -> Result<()> {
        self.check_revision(expected)?;
        let mut next = Self::from_project(snapshot, epoch, self.limits.clone())?;
        next.revision = self.revision + 1;
        *self = next;
        Ok(())
    }
}
