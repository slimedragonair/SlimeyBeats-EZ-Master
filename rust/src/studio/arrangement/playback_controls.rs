//! Live control changes for an already prepared arrangement engine.
use super::*;

impl ArrangementPlayer {
    pub fn set_output_gain_db(&mut self, gain_db: f64) -> Result<()> {
        let mut options = self.options.clone();
        options.output_gain_db = gain_db;
        options.validate()?;
        self.options = options;
        self.output_gain.set([lin(gain_db); 2]);
        Ok(())
    }
    pub fn set_clip_gain(&mut self, clip_id: &str, gain_db: f64) -> Result<()> {
        crate::studio::arrangement::metadata::gain(gain_db)?;
        let index = self
            .plan
            .clips
            .iter()
            .position(|clip| clip.meta.id == clip_id)
            .context("Arrangement clip ID is not present")?;
        self.plan.clips[index].meta.gain_db = gain_db;
        self.plan.clips[index].gain.set([lin(gain_db); 2]);
        for lane in &mut self.document.lanes {
            if let Some(clip) = lane.clips.iter_mut().find(|clip| clip.id == clip_id) {
                clip.gain_db = gain_db;
            }
        }
        Ok(())
    }
    pub fn set_lane_controls(
        &mut self,
        lane_id: &str,
        gain_db: f64,
        pan: f64,
        mute: bool,
        solo: bool,
    ) -> Result<()> {
        crate::studio::arrangement::metadata::gain(gain_db)?;
        ensure!(
            pan.is_finite() && (-1. ..=1.).contains(&pan) && (self.channels() == 2 || pan == 0.),
            "Invalid arrangement lane pan"
        );
        let lane = self
            .document
            .lanes
            .iter_mut()
            .find(|lane| lane.id == lane_id)
            .context("Arrangement lane ID is not present")?;
        lane.gain_db = gain_db;
        lane.pan = pan;
        lane.mute = mute;
        lane.solo = solo;
        let any_solo = self.document.lanes.iter().any(|lane| lane.solo);
        for (meta, voice) in self.document.lanes.iter().zip(&mut self.plan.lanes) {
            voice.gain.set(lane_levels(meta, any_solo));
        }
        Ok(())
    }
    /// Fast coefficient updates retain a fixed target/range. Structural changes
    /// use a prepared replacement, keeping allocation and priming off callbacks.
    pub fn set_eq(&mut self, effect: &ScopedEq) -> Result<()> {
        effect.validate_settings()?;
        let old = self
            .document
            .eq
            .iter_mut()
            .find(|old| old.id == effect.id)
            .context("Scoped EQ ID is not present; prepare a replacement to add effects")?;
        ensure!(
            old.scope == effect.scope && old.range == effect.range,
            "Changing EQ scope/range requires a prepared arrangement replacement"
        );
        *old = effect.clone();
        let processors = match &effect.scope {
            EqScope::Mix {} => &mut self.plan.mix_effects,
            EqScope::Lane { lane_id } => {
                let index = self
                    .document
                    .lanes
                    .iter()
                    .position(|lane| &lane.id == lane_id)
                    .unwrap();
                &mut self.plan.lanes[index].effects
            }
            EqScope::Clip { clip_id } => {
                &mut self
                    .plan
                    .clips
                    .iter_mut()
                    .find(|clip| &clip.meta.id == clip_id)
                    .unwrap()
                    .effects
            }
        };
        processors
            .iter_mut()
            .find(|processor| processor.spec.id == effect.id)
            .unwrap()
            .update(effect);
        Ok(())
    }
    pub fn apply_prepared_eq(
        &mut self,
        mut update: crate::studio::arrangement::PreparedEqUpdate,
    ) -> Result<ScopedEq> {
        self.apply_prepared_eq_in_place(&mut update)
    }
    /// Retain the control container on its caller for off-thread disposal. The
    /// returned old document metadata must also be retired outside the callback.
    pub fn apply_prepared_eq_in_place(
        &mut self,
        update: &mut crate::studio::arrangement::PreparedEqUpdate,
    ) -> Result<ScopedEq> {
        ensure!(
            update.sample_rate == self.sample_rate(),
            "Prepared EQ sample rate differs from the arrangement"
        );
        let effect = &update.spec;
        let old = self
            .document
            .eq
            .iter_mut()
            .find(|old| old.id == effect.id)
            .context("Scoped EQ ID is not present")?;
        ensure!(
            old.scope == effect.scope && old.range == effect.range,
            "Prepared EQ target/range differs"
        );
        ensure!(
            update.document_spec.is_some(),
            "Prepared EQ was already consumed"
        );
        let previous = std::mem::replace(old, update.document_spec.take().unwrap());
        let processors = match &effect.scope {
            EqScope::Mix {} => &mut self.plan.mix_effects,
            EqScope::Lane { lane_id } => {
                let index = self
                    .document
                    .lanes
                    .iter()
                    .position(|lane| &lane.id == lane_id)
                    .unwrap();
                &mut self.plan.lanes[index].effects
            }
            EqScope::Clip { clip_id } => {
                &mut self
                    .plan
                    .clips
                    .iter_mut()
                    .find(|clip| &clip.meta.id == clip_id)
                    .unwrap()
                    .effects
            }
        };
        processors
            .iter_mut()
            .find(|processor| processor.spec.id == effect.id)
            .unwrap()
            .apply_prepared(update);
        Ok(previous)
    }
}
