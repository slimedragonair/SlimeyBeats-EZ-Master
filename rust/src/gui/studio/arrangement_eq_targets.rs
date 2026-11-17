//! Resolve explicit EQ scopes and translate clip sections between song and source time.
use super::arrangement_state::{ArrangementEditor, EqTarget};
use crate::{
    config::default_eq,
    studio::arrangement::{EqScope, FrameRange, ScopedEq},
};
use anyhow::{Context, Result, ensure};

impl ArrangementEditor {
    pub fn eq_target_spec(&self) -> Result<(EqScope, Option<FrameRange>)> {
        let document = self.document().context("Import audio first")?;
        let scope = match self.eq_target {
            EqTarget::Mix | EqTarget::MixRange => EqScope::Mix {},
            EqTarget::Lane | EqTarget::LaneRange => EqScope::Lane {
                lane_id: self.selected_lane.clone().context("Select a track first")?,
            },
            EqTarget::Clip | EqTarget::ClipRange => EqScope::Clip {
                clip_id: self.selected_clip.clone().context("Select a clip first")?,
            },
        };
        let range = if matches!(
            self.eq_target,
            EqTarget::MixRange | EqTarget::LaneRange | EqTarget::ClipRange
        ) {
            let mut range = self.selection.context("Select a time range first")?;
            ensure!(
                range.end_frame > range.start_frame,
                "Select a nonempty time range"
            );
            ensure!(
                range.end_frame <= document.duration_frames(),
                "Time selection exceeds the arrangement"
            );
            ensure!(
                (range.end_frame - range.start_frame) as f64 / document.sample_rate as f64 >= 0.02,
                "Select at least 20 ms for smoothly blended section EQ"
            );
            if let EqScope::Clip { clip_id } = &scope {
                let clip = document.clip(clip_id)?;
                ensure!(
                    range.start_frame >= clip.start_frame && range.end_frame <= clip.end_frame,
                    "Keep the selected range inside the selected clip"
                );
                range.start_frame = range.start_frame - clip.start_frame + clip.source_start_frame;
                range.end_frame = range.end_frame - clip.start_frame + clip.source_start_frame;
            }
            Some(range)
        } else {
            None
        };
        Ok((scope, range))
    }

    pub fn eq_effect(&mut self) -> Result<ScopedEq> {
        let (scope, range) = self.eq_target_spec()?;
        let id = self.eq_id.clone().unwrap_or_else(|| self.fresh_id("eq"));
        self.eq_id = Some(id.clone());
        Ok(ScopedEq {
            id,
            scope,
            range,
            bands: self.eq_bands.clone(),
            output_gain_db: self.eq_output_db,
            bypass: self.eq_bypass,
        })
    }

    pub fn refresh_eq(&mut self) {
        let effect = self.eq_target_spec().ok().and_then(|(scope, range)| {
            self.document().and_then(|document| {
                let matches = |effect: &&ScopedEq| effect.scope == scope && effect.range == range;
                document
                    .eq
                    .iter()
                    .find(|effect| self.eq_id.as_ref() == Some(&effect.id) && matches(effect))
                    .or_else(|| document.eq.iter().rev().find(matches))
                    .cloned()
            })
        });
        self.eq_id = effect.as_ref().map(|effect| effect.id.clone());
        self.eq_bands = effect
            .as_ref()
            .map(|effect| effect.bands.clone())
            .unwrap_or_else(default_eq);
        self.eq_output_db = effect.as_ref().map_or(0., |effect| effect.output_gain_db);
        self.eq_bypass = effect.as_ref().is_some_and(|effect| effect.bypass);
        self.eq_proposal = None;
        self.eq_notice = None;
    }
}
