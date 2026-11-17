//! Native workspace state and worker-result admission; DSP and assets remain shared.
use super::arrangement_state::Inspector;
use super::{
    arrangement_gestures::Gesture,
    arrangement_state::ArrangementEditor,
    arrangement_waveforms::{PreparedWaveform, Waveforms},
};
use crate::{
    audio::Audio,
    dsp::eq_suggestion::EqSuggestion,
    studio::{
        arrangement::{Arrangement, ArrangementPlayer, RenderOptions, ScopedEq},
        arrangement_project::ArrangementSnapshot,
    },
};
use anyhow::{Context, Result, ensure};
use std::sync::Arc;

pub(super) const NATIVE_SOURCE_BUDGET: u64 =
    crate::studio_native::stem_files::DEFAULT_BANK_DECODED_BYTES;

pub(super) struct NativeArrangement {
    pub editor: ArrangementEditor,
    pub waveforms: Waveforms,
    pub gesture: Option<Gesture>,
    pub options: RenderOptions,
    pub reference: Option<Arc<Audio>>,
    pub origin: Option<Arrangement>,
    pub sent_epoch: Option<u64>,
    pub rendered_epoch: Option<u64>,
    pub rendered_audio: Option<Arc<Audio>>,
    pub promote: bool,
    pub position: f64,
    pub after: bool,
    pub legacy_position: f64,
    pub legacy_after: bool,
    pub profile: String,
    pub saved_epoch: Option<u64>,
    pub capture_eq_on_load: bool,
    pub capture_inspector_scroll: Option<f32>,
}
impl Default for NativeArrangement {
    fn default() -> Self {
        Self {
            editor: Default::default(),
            waveforms: Default::default(),
            gesture: None,
            options: RenderOptions {
                max_output_bytes: NATIVE_SOURCE_BUDGET,
                ..Default::default()
            },
            reference: None,
            origin: None,
            sent_epoch: None,
            rendered_epoch: None,
            rendered_audio: None,
            promote: false,
            position: 0.,
            after: true,
            legacy_position: 0.,
            legacy_after: true,
            profile: "neutral".into(),
            saved_epoch: None,
            capture_eq_on_load: false,
            capture_inspector_scroll: None,
        }
    }
}

pub struct PreparedSession {
    pub(super) expected_epoch: u64,
    pub(super) editor: ArrangementEditor,
    pub(super) waveforms: Vec<(String, PreparedWaveform)>,
    pub(super) player: ArrangementPlayer,
    pub(super) reference: Arc<Audio>,
    pub(super) origin: Arrangement,
    pub(super) options: RenderOptions,
    pub(super) loaded_project: bool,
}

pub enum Change {
    ImportPlan(crate::studio_native::stem_bundle::PreparedImport),
    Session(Box<PreparedSession>),
    Player {
        epoch: u64,
        player: Box<ArrangementPlayer>,
        reference: Arc<Audio>,
    },
    Suggestion {
        epoch: u64,
        profile: String,
        target: ScopedEq,
        proposal: Box<EqSuggestion>,
    },
    Rendered {
        epoch: u64,
        audio: Arc<Audio>,
        loudness: f64,
        promote: bool,
    },
    Saved {
        epoch: u64,
        path: std::path::PathBuf,
    },
}

impl Change {
    pub fn durable(&self) -> bool {
        matches!(self, Self::Saved { .. })
    }
}

impl NativeArrangement {
    pub fn snapshot_is_stale(&self, source: Option<&Arc<Audio>>) -> bool {
        source
            .zip(self.rendered_audio.as_ref())
            .is_some_and(|(source, rendered)| {
                Arc::ptr_eq(source, rendered) && self.rendered_epoch != Some(self.editor.epoch)
            })
    }
    pub fn snapshot(&self) -> Result<ArrangementSnapshot> {
        ensure!(
            self.editor.draft.is_none(),
            "Finish the current edit before saving the project"
        );
        let history = self
            .editor
            .history
            .as_ref()
            .context("Import audio first")?
            .snapshot();
        let before = history.undo.last().cloned();
        // This is a cheap capture of already-admitted immutable sources. Disk
        // serialization/PCM validation happens in the background save worker.
        Ok(ArrangementSnapshot {
            history,
            assets: self.editor.assets.clone(),
            asset_names: self.editor.asset_names.clone(),
            render_options: self.options.clone(),
            origin: self.origin.clone(),
            before,
            undo_labels: vec![],
            redo_labels: vec![],
        })
    }

    pub fn accept_suggestion(
        &mut self,
        epoch: u64,
        target: &ScopedEq,
        proposal: EqSuggestion,
    ) -> Result<()> {
        let (scope, range) = self.editor.eq_target_spec()?;
        ensure!(
            epoch == self.editor.epoch && scope == target.scope && range == target.range,
            "The EQ target changed while measuring. Suggest again for the current target."
        );
        self.editor.eq_proposal = Some(proposal);
        self.editor.eq_notice =
            Some("The pale curve is a proposal. Apply curve to hear it on this target.".into());
        Ok(())
    }
}

impl super::state::Studio {
    #[doc(hidden)]
    pub fn capture_arrangement_inspector_at(&mut self, offset: f32) {
        self.arrangement.capture_inspector_scroll = Some(offset.max(0.));
    }
    /// Renderer verification only; inspect a validated project without changing its recipe.
    #[doc(hidden)]
    pub fn capture_arrangement_eq_on_load(&mut self) {
        self.arrangement.capture_eq_on_load = true;
    }
    pub(in crate::gui) fn arrangement_eq_open(&self) -> bool {
        self.page == super::state::Page::Tracks
            && (self.arrangement.editor.inspector == Inspector::Eq)
    }
    pub(in crate::gui) fn arrangement_counts(&self) -> (usize, usize, usize) {
        self.arrangement
            .editor
            .document()
            .map(|document| {
                (
                    document.lanes.len(),
                    document.lanes.iter().map(|lane| lane.clips.len()).sum(),
                    document.eq.len(),
                )
            })
            .unwrap_or_default()
    }
}
