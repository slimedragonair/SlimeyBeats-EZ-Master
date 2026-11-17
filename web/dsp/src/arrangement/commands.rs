use super::*;
use mastercraft::studio::arrangement_session::edit_label;

impl BrowserArrangement {
    pub(super) fn edit_checked(&mut self, expected: u64, json: &str) -> Result<String> {
        let edits = edits(json)?;
        let can_update = self.player_side == "after"
            && self.player_revision == Some(expected)
            && edits.iter().all(|edit| match edit {
                Edit::SetLane { .. } | Edit::SetClipGain { .. } | Edit::SetClipName { .. } => true,
                Edit::SetEq { effect } => self.session.document().eq.iter().any(|old| {
                    old.id == effect.id && old.scope == effect.scope && old.range == effect.range
                }),
                _ => false,
            });
        self.session.edit(expected, &edits, &edit_label(&edits))?;
        if can_update {
            let player = self
                .player
                .as_mut()
                .expect("Prepared player revision has a player");
            for edit in &edits {
                match edit {
                    Edit::SetLane { lane_id, .. } => {
                        let lane = self.session.document().lane(lane_id)?;
                        player.set_lane_controls(
                            lane_id,
                            lane.gain_db,
                            lane.pan,
                            lane.mute,
                            lane.solo,
                        )?;
                    }
                    Edit::SetClipGain { clip_id, gain_db } => {
                        player.set_clip_gain(clip_id, *gain_db)?
                    }
                    Edit::SetEq { effect } => player.set_eq(effect)?,
                    Edit::SetClipName { .. } => {}
                    _ => unreachable!("Only fixed-geometry live controls are admitted here"),
                }
            }
            self.player_revision = Some(self.session.revision());
        } else if self.session.revision() != expected {
            self.player_revision = None;
        }
        self.snapshot()
    }
}

#[wasm_bindgen]
impl BrowserArrangement {
    pub fn edit(&mut self, expected: u64, json: &str) -> Result<String, JsValue> {
        self.edit_checked(expected, json).map_err(failure)
    }
    pub fn history(&mut self, expected: u64, redo: bool) -> Result<String, JsValue> {
        let work = (|| -> Result<String> {
            if redo {
                self.session.redo(expected)?;
            } else {
                self.session.undo(expected)?;
            }
            self.player_revision = None;
            self.snapshot()
        })();
        work.map_err(failure)
    }
}
