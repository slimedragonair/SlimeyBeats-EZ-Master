//! Recover the committed clip recipe through the existing private attachment pool.
use super::super::{
    arrangement_runtime::NativeArrangement,
    arrangement_state::ArrangementEditor,
    arrangement_waveforms::{PreparedWaveform, Waveforms},
    state::{Page, Studio},
};
use crate::{
    audio::Audio,
    gui::{recovery_capture::Attachments, recovery_document::SavedArrangement},
    studio::{
        arrangement::{
            ArrangementHistory, ArrangementPlayer, AssetLibrary, DEFAULT_HISTORY_METADATA_BYTES,
        },
        arrangement_project::ArrangementSnapshot,
    },
};
use anyhow::{Context, Result, ensure};
use std::{
    hash::{Hash, Hasher},
    io,
    sync::{Arc, atomic::Ordering},
};

pub(in crate::gui) struct PreparedArrangement {
    workspace: NativeArrangement,
    player: Option<ArrangementPlayer>,
}

impl Studio {
    pub(in crate::gui) fn has_recovery_arrangement(&self) -> bool {
        self.arrangement.editor.history.is_some()
    }

    pub(in crate::gui) fn capture_recovery_arrangement(
        &self,
        attachments: &mut Attachments,
    ) -> Option<SavedArrangement> {
        let workspace = &self.arrangement;
        let editor = &workspace.editor;
        let history = editor.history.as_ref()?;
        Some(SavedArrangement {
            version: 1,
            // document() includes a live gesture. History is the committed recipe.
            history: history.snapshot(),
            history_budget_bytes: history.metadata_budget_bytes(),
            assets: editor
                .assets
                .iter()
                .map(|(id, audio)| (id.clone(), attachments.add(audio)))
                .collect(),
            asset_names: editor.asset_names.clone(),
            options: workspace.options.clone(),
            reference: workspace
                .reference
                .as_ref()
                .map(|audio| attachments.add(audio)),
            origin: workspace.origin.clone(),
            position: if self.player.has_live_arrangement() {
                self.player.seconds()
            } else {
                workspace.position
            },
            profile: workspace.profile.clone(),
        })
    }

    pub(in crate::gui) fn hash_recovery_arrangement(&self, hash: &mut impl Hasher) -> Result<()> {
        self.page.hash(hash);
        let workspace = &self.arrangement;
        let editor = &workspace.editor;
        let Some(history) = &editor.history else {
            false.hash(hash);
            return Ok(());
        };
        true.hash(hash);
        // No snapshot clone, PCM scan, draft epoch or playhead in the autosave probe.
        serde_json::to_writer(
            HashWriter(hash),
            &(
                history.revision(),
                history.current(),
                history.undo_len(),
                history.redo_len(),
                history.metadata_budget_bytes(),
                &editor.asset_names,
                &workspace.options,
                &workspace.origin,
                &workspace.profile,
            ),
        )?;
        for (id, audio) in &editor.assets {
            id.hash(hash);
            (Arc::as_ptr(audio) as usize).hash(hash);
        }
        workspace
            .reference
            .as_ref()
            .map(|audio| Arc::as_ptr(audio) as usize)
            .hash(hash);
        Ok(())
    }

    pub(in crate::gui) fn prepare_recovery_arrangement(
        saved: SavedArrangement,
        attachments: &[Arc<Audio>],
        cancel: &dyn Fn() -> bool,
    ) -> Result<PreparedArrangement> {
        ensure!(!cancel(), "Cancelled");
        ensure!(
            saved.version == 1,
            "Unsupported recovered arrangement version"
        );
        ensure!(
            saved.history_budget_bytes <= DEFAULT_HISTORY_METADATA_BYTES,
            "Recovered arrangement history exceeds its metadata budget"
        );
        ensure!(
            saved.position.is_finite() && saved.position >= 0.,
            "Invalid recovered arrangement playhead"
        );
        ensure!(
            !saved.profile.trim().is_empty()
                && saved.profile.len() <= 512
                && !saved.profile.chars().any(char::is_control),
            "Invalid recovered tonal profile"
        );
        let get = |index: usize| {
            attachments
                .get(index)
                .cloned()
                .context("Recovered arrangement audio reference is missing")
        };
        let assets = saved
            .assets
            .into_iter()
            .map(|(id, index)| Ok((id, get(index)?)))
            .collect::<Result<AssetLibrary>>()?;
        let snapshot = ArrangementSnapshot {
            history: saved.history,
            assets,
            asset_names: saved.asset_names,
            render_options: saved.options,
            origin: saved.origin,
            before: None,
            undo_labels: vec![],
            redo_labels: vec![],
        };
        snapshot.validate()?;
        ensure!(!cancel(), "Cancelled");
        let reference = saved.reference.map(get).transpose()?;
        if let Some(reference) = &reference {
            ensure!(
                reference.sample_rate == snapshot.document().sample_rate
                    && reference.channels == snapshot.document().channels
                    && !reference.samples.is_empty()
                    && reference.samples.len().is_multiple_of(reference.channels)
                    && reference.samples.iter().all(|sample| sample.is_finite()),
                "Recovered initial-import comparison has invalid PCM or project layout"
            );
            if let Some(origin) = &snapshot.origin {
                ensure!(
                    reference.frames() as u64 == origin.duration_frames(),
                    "Recovered initial-import comparison does not match its origin"
                );
            }
        }
        let history = ArrangementHistory::from_snapshot_with_budget(
            snapshot.history,
            &snapshot.assets,
            saved.history_budget_bytes,
        )?;
        let mut editor = ArrangementEditor::default();
        editor.history = Some(history);
        editor.assets = snapshot.assets;
        editor.asset_names = snapshot.asset_names;
        editor.epoch = 1;
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
        let mut waveforms = Waveforms::default();
        for (id, audio) in &editor.assets {
            ensure!(!cancel(), "Cancelled");
            waveforms.insert(id.clone(), PreparedWaveform::new(audio.clone())?);
        }
        let document = editor
            .document()
            .context("Recovered arrangement has no committed document")?;
        let position = saved
            .position
            .min(document.duration_frames() as f64 / document.sample_rate as f64);
        let player = reference
            .as_ref()
            .map(|_| {
                ArrangementPlayer::new_cancellable(
                    document,
                    &editor.assets,
                    &snapshot.render_options,
                    cancel,
                )
            })
            .transpose()?;
        ensure!(!cancel(), "Cancelled");
        Ok(PreparedArrangement {
            player,
            workspace: NativeArrangement {
                editor,
                waveforms,
                options: snapshot.render_options,
                reference,
                origin: snapshot.origin,
                position,
                profile: saved.profile,
                ..Default::default()
            },
        })
    }

    pub(super) fn adopt_recovery_arrangement(
        &mut self,
        prepared: Option<PreparedArrangement>,
    ) -> bool {
        let Some(prepared) = prepared else {
            self.arrangement = Default::default();
            return false;
        };
        self.arrangement = prepared.workspace;
        if self.page != Page::Tracks {
            return false;
        }
        if let Some(player) = prepared.player
            && let Some(reference) = &self.arrangement.reference
        {
            match self.player.load_arrangement(reference.clone(), player) {
                Ok(()) => self.arrangement.sent_epoch = Some(self.arrangement.editor.epoch),
                Err(error) => {
                    self.error = Some(format!(
                        "Tracks recovered; playback device is unavailable: {error:#}"
                    ))
                }
            }
        }
        self.player.controls.after.store(true, Ordering::Relaxed);
        self.player.controls.matched.store(false, Ordering::Relaxed);
        true
    }
}

struct HashWriter<'a, H>(&'a mut H);
impl<H: Hasher> io::Write for HashWriter<'_, H> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "recovery_arrangement_tests.rs"]
mod tests;
