//! Native editor selection and metadata history; immutable PCM belongs to the asset pool.
use crate::{
    audio::Audio,
    config::{EqBand, default_eq},
    studio::arrangement::{
        Arrangement, ArrangementHistory, AssetLibrary, Clip, Edit, FrameRange, Lane,
    },
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, sync::Arc};

const HISTORY_STEPS: usize = 32;
const MAX_SOURCE_BYTES: u64 = super::arrangement_runtime::NATIVE_SOURCE_BUDGET;

#[derive(Clone, Default)]
pub(super) struct LaneInit {
    pub gain_db: f64,
    pub pan: f64,
    pub mute: bool,
    pub solo: bool,
    pub clip_gain_db: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum EqTarget {
    #[default]
    Mix,
    Lane,
    Clip,
    MixRange,
    LaneRange,
    ClipRange,
}

pub(super) struct ArrangementEditor {
    pub history: Option<ArrangementHistory>,
    pub assets: AssetLibrary,
    pub asset_names: BTreeMap<String, String>,
    pub selected_lane: Option<String>,
    pub selected_clip: Option<String>,
    pub selection: Option<FrameRange>,
    pub view_start: f64,
    pub view_end: f64,
    pub move_tool: bool,
    pub eq_inspector: bool,
    pub eq_target: EqTarget,
    pub eq_bands: Vec<EqBand>,
    pub eq_output_db: f64,
    pub eq_bypass: bool,
    pub eq_selected_band: usize,
    pub eq_proposal: Option<crate::dsp::eq_suggestion::EqSuggestion>,
    pub eq_notice: Option<String>,
    pub eq_id: Option<String>,
    pub lane_name_draft: Option<(String, String)>,
    pub clip_name_draft: Option<(String, String)>,
    pub epoch: u64,
    pub(super) draft: Option<Arrangement>,
    draft_edit: Option<Edit>,
    next_id: u64,
}

impl Default for ArrangementEditor {
    fn default() -> Self {
        Self {
            history: None,
            assets: AssetLibrary::new(),
            asset_names: BTreeMap::new(),
            selected_lane: None,
            selected_clip: None,
            selection: None,
            view_start: 0.,
            view_end: 30.,
            move_tool: false,
            eq_inspector: false,
            eq_target: EqTarget::Mix,
            eq_bands: default_eq(),
            eq_output_db: 0.,
            eq_bypass: false,
            eq_selected_band: 6,
            eq_proposal: None,
            eq_notice: None,
            eq_id: None,
            lane_name_draft: None,
            clip_name_draft: None,
            epoch: 0,
            draft: None,
            draft_edit: None,
            next_id: 1,
        }
    }
}

impl ArrangementEditor {
    pub fn commit_names(&mut self) -> Result<()> {
        self.commit_lane_name()?;
        self.commit_clip_name()
    }
    pub fn restore(
        history: crate::studio::arrangement::HistorySnapshot,
        assets: AssetLibrary,
        asset_names: BTreeMap<String, String>,
        epoch: u64,
    ) -> Result<Self> {
        let history = Some(ArrangementHistory::from_snapshot(history, &assets)?);
        Ok(Self {
            history,
            assets,
            asset_names,
            epoch,
            ..Self::default()
        })
    }
    pub fn fork_for_import(&self) -> Result<Self> {
        ensure!(
            self.draft.is_none(),
            "Finish the current edit before importing tracks"
        );
        let history = self
            .history
            .as_ref()
            .map(|history| ArrangementHistory::from_snapshot(history.snapshot(), &self.assets))
            .transpose()?;
        Ok(Self {
            history,
            assets: self.assets.clone(),
            asset_names: self.asset_names.clone(),
            selected_lane: self.selected_lane.clone(),
            selected_clip: self.selected_clip.clone(),
            selection: self.selection,
            view_start: self.view_start,
            view_end: self.view_end,
            move_tool: self.move_tool,
            eq_inspector: self.eq_inspector,
            eq_target: self.eq_target,
            eq_bands: self.eq_bands.clone(),
            eq_output_db: self.eq_output_db,
            eq_bypass: self.eq_bypass,
            eq_selected_band: self.eq_selected_band,
            epoch: self.epoch,
            next_id: self.next_id,
            ..Self::default()
        })
    }
    pub fn document(&self) -> Option<&Arrangement> {
        self.draft
            .as_ref()
            .or_else(|| self.history.as_ref().map(ArrangementHistory::current))
    }

    pub fn fresh_id(&mut self, prefix: &str) -> String {
        loop {
            let id = format!("{prefix}_{}", self.next_id);
            self.next_id += 1;
            let used =
                self.assets.contains_key(&id)
                    || self.document().is_some_and(|document| {
                        document.lanes.iter().any(|lane| {
                            lane.id == id || lane.clips.iter().any(|clip| clip.id == id)
                        }) || document.eq.iter().any(|effect| effect.id == id)
                    });
            if !used {
                return id;
            }
        }
    }

    /// Decoding/conforming happens in a worker first. A failed batch changes neither pool nor history.
    pub fn add_sources_with_controls(
        &mut self,
        sources: Vec<(String, Arc<Audio>, LaneInit)>,
    ) -> Result<()> {
        ensure!(!sources.is_empty(), "Choose at least one audio file");
        ensure!(
            self.draft.is_none(),
            "Finish the current gesture before importing audio"
        );
        let rate = self
            .document()
            .map(|document| document.sample_rate)
            .unwrap_or(sources[0].1.sample_rate);
        let mut assets = self.assets.clone();
        let mut names = self.asset_names.clone();
        let mut edits = Vec::new();
        let mut last_selection = None;
        for (name, audio, control) in sources {
            ensure!(
                audio.sample_rate == rate,
                "Conform imported audio to the project sample rate first"
            );
            ensure!(
                (1..=2).contains(&audio.channels) && audio.frames() > 0,
                "Import mono or stereo audio with at least one frame"
            );
            let source_id = self.fresh_id("source");
            let lane_id = self.fresh_id("track");
            let clip_id = self.fresh_id("clip");
            let frames = audio.frames() as u64;
            assets.insert(source_id.clone(), audio);
            names.insert(source_id.clone(), name.clone());
            let mut lane = Lane::new(lane_id.clone(), name);
            lane.gain_db = control.gain_db;
            lane.pan = control.pan;
            lane.mute = control.mute;
            lane.solo = control.solo;
            // Deserializing the metadata keeps optional clip fields at their shared defaults.
            let clip: Clip = serde_json::from_value(serde_json::json!({
                "id": clip_id, "source_id": source_id, "start_frame": 0,
                "source_start_frame": 0, "end_frame": frames,
                "gain_db": control.clip_gain_db, "fade_in_frames": 0, "fade_out_frames": 0
            }))?;
            edits.push(Edit::AddLane { lane });
            edits.push(Edit::AddClip {
                lane_id: lane_id.clone(),
                clip,
            });
            last_selection = Some((lane_id, clip_id));
        }
        let mut unique = std::collections::BTreeSet::new();
        let mut bytes = 0_u64;
        for audio in assets.values() {
            if unique.insert(Arc::as_ptr(audio) as usize) {
                bytes = bytes
                    .checked_add(
                        (audio.samples.len() as u64)
                            .checked_mul(4)
                            .context("Source byte count overflow")?,
                    )
                    .context("Source byte count overflow")?;
            }
        }
        ensure!(
            bytes <= MAX_SOURCE_BYTES,
            "Arrangement sources exceed the 4 GiB native session budget"
        );
        if let Some(history) = &mut self.history {
            history.apply_batch(&edits, &assets)?;
        } else {
            let mut history =
                ArrangementHistory::new(Arrangement::new(rate, 2), &assets, HISTORY_STEPS)?;
            history.apply_batch(&edits, &assets)?;
            self.history = Some(history);
        }
        self.assets = assets;
        self.asset_names = names;
        if let Some((lane, clip)) = last_selection {
            self.selected_lane = Some(lane);
            self.selected_clip = Some(clip);
        }
        self.epoch += 1;
        self.fit();
        self.sync_selection();
        self.refresh_eq();
        Ok(())
    }

    pub fn select_clip(&mut self, id: &str) -> Result<()> {
        self.commit_lane_name()?;
        self.commit_clip_name()?;
        let document = self.document().context("Import audio first")?;
        let lane = document
            .lanes
            .iter()
            .find(|lane| lane.clips.iter().any(|clip| clip.id == id))
            .context("That clip is no longer present")?;
        let clip = document.clip(id)?;
        let selection = FrameRange {
            start_frame: clip.start_frame,
            end_frame: clip.end_frame,
        };
        self.selected_lane = Some(lane.id.clone());
        self.selected_clip = Some(id.into());
        self.selection = Some(selection);
        self.refresh_eq();
        Ok(())
    }

    pub fn select_lane(&mut self, id: &str) -> Result<()> {
        self.commit_names()?;
        let document = self.document().context("Import audio first")?;
        let lane = document.lane(id)?;
        let clip = self
            .selected_clip
            .as_ref()
            .and_then(|selected| lane.clips.iter().find(|clip| &clip.id == selected))
            .or_else(|| lane.clips.first());
        let clip_id = clip.map(|clip| clip.id.clone());
        let selection =
            if self.selected_lane.as_deref() == Some(id) && self.selected_clip == clip_id {
                self.selection
            } else {
                clip.map(|clip| FrameRange {
                    start_frame: clip.start_frame,
                    end_frame: clip.end_frame,
                })
            };
        self.selected_lane = Some(id.into());
        self.selected_clip = clip_id;
        self.selection = selection;
        self.refresh_eq();
        Ok(())
    }

    pub fn commit_lane_name(&mut self) -> Result<()> {
        let Some((id, name)) = self.lane_name_draft.clone() else {
            return Ok(());
        };
        let previous = self
            .document()
            .and_then(|document| document.lane(&id).ok())
            .map(|lane| lane.name.clone());
        if previous.is_some_and(|previous| previous != name) {
            self.apply(Edit::SetLane {
                lane_id: id,
                name: Some(name),
                gain_db: None,
                pan: None,
                mute: None,
                solo: None,
            })?;
        }
        self.lane_name_draft = None;
        Ok(())
    }

    pub fn commit_clip_name(&mut self) -> Result<()> {
        let Some((id, name)) = self.clip_name_draft.clone() else {
            return Ok(());
        };
        let previous = self
            .document()
            .and_then(|document| document.clip(&id).ok())
            .map(|clip| clip.name.clone());
        let name = (!name.trim().is_empty()).then_some(name);
        if previous.is_some_and(|previous| previous != name) {
            self.apply(Edit::SetClipName { clip_id: id, name })?;
        }
        self.clip_name_draft = None;
        Ok(())
    }

    pub fn apply(&mut self, edit: Edit) -> Result<()> {
        self.cancel_draft();
        self.history
            .as_mut()
            .context("Import audio first")?
            .apply(&edit, &self.assets)?;
        self.epoch += 1;
        self.sync_selection();
        self.refresh_eq();
        Ok(())
    }

    pub fn preview_edit(&mut self, edit: Edit) -> Result<()> {
        let current = self
            .history
            .as_ref()
            .context("Import audio first")?
            .current();
        let next = crate::studio::arrangement::apply_edit(current, &edit, &self.assets)?;
        if self.draft.as_ref() == Some(&next) {
            return Ok(());
        }
        self.draft = Some(next);
        self.draft_edit = Some(edit);
        self.epoch += 1;
        Ok(())
    }

    pub fn commit_draft(&mut self) -> Result<()> {
        let Some(edit) = self.draft_edit.take() else {
            return Ok(());
        };
        self.draft = None;
        self.apply(edit)
    }

    pub fn cancel_draft(&mut self) {
        if self.draft.take().is_some() {
            self.epoch += 1;
            self.refresh_eq();
        }
        self.draft_edit = None;
    }

    pub fn history_step(&mut self, redo: bool) -> Result<()> {
        self.cancel_draft();
        let history = self.history.as_mut().context("Import audio first")?;
        if redo {
            history.redo()?;
        } else {
            history.undo()?;
        }
        self.epoch += 1;
        self.sync_selection();
        self.refresh_eq();
        Ok(())
    }

    pub fn fit(&mut self) {
        if let Some(document) = self.document() {
            let duration = document.duration_frames() as f64 / document.sample_rate as f64;
            self.view_start = 0.;
            self.view_end = duration.max(1.);
        }
    }

    fn sync_selection(&mut self) {
        let Some(document) = self.document() else {
            return;
        };
        let lane = self
            .selected_lane
            .as_ref()
            .and_then(|id| document.lane(id).ok())
            .or_else(|| document.lanes.first());
        let lane_id = lane.map(|lane| lane.id.clone());
        let clip_id = self
            .selected_clip
            .as_ref()
            .filter(|id| lane.is_some_and(|lane| lane.clips.iter().any(|clip| &clip.id == *id)))
            .cloned()
            .or_else(|| {
                lane.and_then(|lane| lane.clips.first())
                    .map(|clip| clip.id.clone())
            });
        self.selected_lane = lane_id;
        self.selected_clip = clip_id;
    }
}
