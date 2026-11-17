use super::{ArrangementSession, AssetInfo, OwnedAsset, SessionSnapshot, identifier, label};
use crate::studio::arrangement::{Edit, check_metadata};
use anyhow::{Context, Result, ensure};

pub fn edit_label(edits: &[Edit]) -> String {
    let label = match edits.first() {
        Some(Edit::AddLane { .. }) => "Add track",
        Some(Edit::RemoveLane { .. }) => "Remove track",
        Some(Edit::ReorderLane { .. }) => "Reorder track",
        Some(Edit::SetLane { .. }) => "Update track",
        Some(Edit::AddClip { .. }) => "Add clip",
        Some(Edit::MoveClip { .. }) => "Move clip",
        Some(Edit::TrimClip { .. }) => "Trim clip",
        Some(Edit::SplitClip { .. }) => "Split clip",
        Some(Edit::DuplicateClip { .. }) => "Duplicate clip",
        Some(Edit::DeleteClip { .. }) => "Delete clip",
        Some(Edit::SetClipName { .. }) => "Rename clip",
        Some(Edit::SetClipGain { .. }) => "Update clip gain",
        Some(Edit::SetClipFades { .. }) => "Update clip fades",
        Some(Edit::SetEq { .. }) => "Update scoped EQ",
        Some(Edit::RemoveEq { .. }) => "Remove scoped EQ",
        None => "Edit arrangement",
    };
    if edits.len() > 1 {
        format!("{label} ({} actions)", edits.len())
    } else {
        label.into()
    }
}

impl ArrangementSession {
    pub fn edit(
        &mut self,
        expected: u64,
        edits: &[Edit],
        action_label: &str,
    ) -> Result<SessionSnapshot> {
        self.check_revision(expected)?;
        label(action_label)?;
        check_metadata(&edits)?;
        let before = self.document().clone();
        let catalog = &self.catalog;
        self.history
            .apply_batch_checked(edits, &self.assets, |document| {
                check_metadata(&(document, catalog))
            })?;
        if self.document() != &before {
            self.committed(before, action_label);
        }
        Ok(self.read())
    }

    pub fn add_assets(
        &mut self,
        expected: u64,
        additions: Vec<OwnedAsset>,
        edits: &[Edit],
        action_label: &str,
    ) -> Result<SessionSnapshot> {
        self.check_revision(expected)?;
        label(action_label)?;
        ensure!(
            !additions.is_empty(),
            "Choose at least one arrangement asset"
        );
        let mut assets = self.assets.clone();
        let mut catalog = self.catalog.clone();
        let mut bytes = assets.values().try_fold(0_u64, |total, audio| {
            total
                .checked_add(audio.samples.len() as u64 * 4)
                .context("Arrangement asset size overflow")
        })?;
        for asset in additions {
            identifier(&asset.id)?;
            label(&asset.name)?;
            ensure!(
                !assets.contains_key(&asset.id),
                "Arrangement asset ID already exists"
            );
            let audio = &asset.audio;
            ensure!(
                audio.sample_rate == self.document().sample_rate
                    && (audio.channels == self.document().channels
                        || (audio.channels == 1 && self.document().channels == 2)),
                "Conform this source explicitly to the project rate/channels before import"
            );
            ensure!(
                !audio.samples.is_empty()
                    && (1..=2).contains(&audio.channels)
                    && audio.samples.len().is_multiple_of(audio.channels)
                    && audio.samples.iter().all(|sample| sample.is_finite()),
                "Arrangement asset contains invalid PCM"
            );
            bytes = bytes
                .checked_add(audio.samples.len() as u64 * 4)
                .context("Arrangement asset size overflow")?;
            ensure!(
                bytes <= self.limits.max_decoded_bytes,
                "Arrangement assets exceed the decoded memory budget"
            );
            catalog.insert(
                asset.id.clone(),
                AssetInfo {
                    id: asset.id.clone(),
                    name: asset.name,
                    frames: audio.frames() as u64,
                    sample_rate: audio.sample_rate,
                    channels: audio.channels,
                },
            );
            assets.insert(asset.id, asset.audio);
        }
        check_metadata(&edits)?;
        let before = self.document().clone();
        self.history
            .apply_batch_checked(edits, &assets, |document| {
                check_metadata(&(document, &catalog))
            })?;
        self.assets = assets;
        self.catalog = catalog;
        if self.origin.lanes.is_empty() {
            self.origin = self.document().clone();
        }
        if self.document() != &before {
            self.committed(before, action_label);
        } else {
            // Adding an unused immutable source changes the library, not timeline history.
            self.revision += 1;
        }
        Ok(self.read())
    }

    pub fn undo(&mut self, expected: u64) -> Result<SessionSnapshot> {
        self.check_revision(expected)?;
        let action = self
            .undo_labels
            .last()
            .context("Arrangement undo label is unavailable")?
            .clone();
        let before = self.document().clone();
        self.history.undo()?;
        self.undo_labels.pop();
        self.redo_labels.push(action);
        self.trim_history_labels();
        self.before = before;
        self.revision += 1;
        Ok(self.read())
    }

    pub fn redo(&mut self, expected: u64) -> Result<SessionSnapshot> {
        self.check_revision(expected)?;
        let action = self
            .redo_labels
            .last()
            .context("Arrangement redo label is unavailable")?
            .clone();
        let before = self.document().clone();
        self.history.redo()?;
        self.redo_labels.pop();
        self.undo_labels.push(action);
        self.trim_history_labels();
        self.before = before;
        self.revision += 1;
        Ok(self.read())
    }

    fn committed(&mut self, before: crate::studio::arrangement::Arrangement, action_label: &str) {
        self.before = before;
        self.undo_labels.push(action_label.into());
        if self.undo_labels.len() > self.limits.history_capacity {
            self.undo_labels.remove(0);
        }
        self.redo_labels.clear();
        self.trim_history_labels();
        self.revision += 1;
    }

    fn trim_history_labels(&mut self) {
        let undo = self
            .undo_labels
            .len()
            .saturating_sub(self.history.undo_len());
        let redo = self
            .redo_labels
            .len()
            .saturating_sub(self.history.redo_len());
        self.undo_labels.drain(..undo);
        self.redo_labels.drain(..redo);
        debug_assert_eq!(self.undo_labels.len(), self.history.undo_len());
        debug_assert_eq!(self.redo_labels.len(), self.history.redo_len());
    }
}
