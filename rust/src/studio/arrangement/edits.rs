use super::{Arrangement, AssetLibrary, Clip, EqScope, FrameRange, Lane, ScopedEq};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub enum Edit {
    AddLane {
        lane: Lane,
    },
    RemoveLane {
        lane_id: String,
    },
    ReorderLane {
        lane_id: String,
        index: usize,
    },
    SetLane {
        lane_id: String,
        name: Option<String>,
        gain_db: Option<f64>,
        pan: Option<f64>,
        mute: Option<bool>,
        solo: Option<bool>,
    },
    AddClip {
        lane_id: String,
        clip: Clip,
    },
    MoveClip {
        clip_id: String,
        lane_id: String,
        start_frame: u64,
    },
    TrimClip {
        clip_id: String,
        start_frame: u64,
        source_start_frame: u64,
        end_frame: u64,
    },
    SplitClip {
        clip_id: String,
        at_frame: u64,
        new_clip_id: String,
    },
    DuplicateClip {
        clip_id: String,
        new_clip_id: String,
        lane_id: String,
        start_frame: u64,
    },
    DeleteClip {
        clip_id: String,
    },
    SetClipGain {
        clip_id: String,
        gain_db: f64,
    },
    SetClipName {
        clip_id: String,
        name: Option<String>,
    },
    SetClipFades {
        clip_id: String,
        fade_in_frames: u64,
        fade_out_frames: u64,
    },
    SetEq {
        effect: ScopedEq,
    },
    RemoveEq {
        eq_id: String,
    },
}

pub fn apply_edit(
    current: &Arrangement,
    edit: &Edit,
    assets: &AssetLibrary,
) -> Result<Arrangement> {
    apply_edits(current, std::slice::from_ref(edit), assets)
}
pub fn apply_edits(
    current: &Arrangement,
    edits: &[Edit],
    assets: &AssetLibrary,
) -> Result<Arrangement> {
    current.validate(assets)?;
    super::budget::check(&edits)?;
    let mut next = current.clone();
    for edit in edits {
        mutate(&mut next, edit)?;
        next.validate(assets)?;
    }
    next.validate(assets)?;
    Ok(next)
}
fn lane_mut<'a>(value: &'a mut Arrangement, id: &str) -> Result<&'a mut Lane> {
    value
        .lanes
        .iter_mut()
        .find(|lane| lane.id == id)
        .context("Arrangement lane ID is not present")
}
fn clip_mut<'a>(value: &'a mut Arrangement, id: &str) -> Result<&'a mut Clip> {
    value
        .lanes
        .iter_mut()
        .flat_map(|lane| &mut lane.clips)
        .find(|clip| clip.id == id)
        .context("Arrangement clip ID is not present")
}
fn remove_clip(value: &mut Arrangement, id: &str) -> Result<Clip> {
    for lane in &mut value.lanes {
        if let Some(index) = lane.clips.iter().position(|clip| clip.id == id) {
            return Ok(lane.clips.remove(index));
        }
    }
    anyhow::bail!("Arrangement clip ID is not present")
}
fn retain_effects(value: &mut Arrangement) {
    let lanes: Vec<_> = value.lanes.iter().map(|lane| lane.id.clone()).collect();
    let clips: Vec<_> = value
        .lanes
        .iter()
        .flat_map(|lane| &lane.clips)
        .map(|clip| clip.id.clone())
        .collect();
    let duration = value.duration_frames();
    value.eq.retain_mut(|effect| {
        if match &effect.scope {
            EqScope::Lane { lane_id } => !lanes.contains(lane_id),
            EqScope::Clip { clip_id } => !clips.contains(clip_id),
            _ => false,
        } {
            return false;
        }
        if let Some(range) = &mut effect.range {
            if matches!(effect.scope, EqScope::Clip { .. }) {
                return true;
            }
            let bounds = FrameRange {
                start_frame: 0,
                end_frame: duration,
            };
            range.start_frame = range.start_frame.max(bounds.start_frame);
            range.end_frame = range.end_frame.min(bounds.end_frame);
            if range.start_frame >= range.end_frame {
                return false;
            }
        }
        true
    });
}
fn mutate(value: &mut Arrangement, edit: &Edit) -> Result<()> {
    match edit {
        Edit::AddLane { lane } => value.lanes.push(lane.clone()),
        Edit::RemoveLane { lane_id } => {
            let index = value
                .lanes
                .iter()
                .position(|lane| &lane.id == lane_id)
                .context("Arrangement lane ID is not present")?;
            value.lanes.remove(index);
            retain_effects(value);
        }
        Edit::ReorderLane { lane_id, index } => {
            ensure!(
                *index < value.lanes.len(),
                "Lane order index is out of bounds"
            );
            let old = value
                .lanes
                .iter()
                .position(|lane| &lane.id == lane_id)
                .context("Arrangement lane ID is not present")?;
            let lane = value.lanes.remove(old);
            value.lanes.insert(*index, lane);
        }
        Edit::SetLane {
            lane_id,
            name,
            gain_db,
            pan,
            mute,
            solo,
        } => {
            let lane = lane_mut(value, lane_id)?;
            if let Some(v) = name {
                lane.name = v.clone();
            }
            if let Some(v) = gain_db {
                lane.gain_db = *v;
            }
            if let Some(v) = pan {
                lane.pan = *v;
            }
            if let Some(v) = mute {
                lane.mute = *v;
            }
            if let Some(v) = solo {
                lane.solo = *v;
            }
        }
        Edit::AddClip { lane_id, clip } => lane_mut(value, lane_id)?.clips.push(clip.clone()),
        Edit::MoveClip {
            clip_id,
            lane_id,
            start_frame,
        } => {
            let mut clip = remove_clip(value, clip_id)?;
            clip.end_frame = start_frame
                .checked_add(clip.duration_frames())
                .context("Moved clip coordinates overflow")?;
            clip.start_frame = *start_frame;
            lane_mut(value, lane_id)?.clips.push(clip);
        }
        Edit::TrimClip {
            clip_id,
            start_frame,
            source_start_frame,
            end_frame,
        } => {
            let clip = clip_mut(value, clip_id)?;
            ensure!(start_frame < end_frame, "Trim must retain a nonempty clip");
            clip.start_frame = *start_frame;
            clip.source_start_frame = *source_start_frame;
            clip.end_frame = *end_frame;
            clip.fade_in_frames = clip.fade_in_frames.min(clip.duration_frames());
            clip.fade_out_frames = clip.fade_out_frames.min(clip.duration_frames());
            retain_effects(value);
        }
        Edit::SplitClip {
            clip_id,
            at_frame,
            new_clip_id,
        } => {
            let original = value.clip(clip_id)?.clone();
            ensure!(
                *at_frame > original.start_frame && *at_frame < original.end_frame,
                "Split must be strictly within the clip"
            );
            let lane_id = value
                .lanes
                .iter()
                .find(|lane| lane.clips.iter().any(|clip| &clip.id == clip_id))
                .unwrap()
                .id
                .clone();
            let mut right = original.clone();
            right.id = new_clip_id.clone();
            right.start_frame = *at_frame;
            right.source_start_frame = original
                .source_start_frame
                .checked_add(at_frame - original.start_frame)
                .context("Split source overflow")?;
            right.fade_in_frames = 0;
            right.fade_out_frames = right.fade_out_frames.min(right.duration_frames());
            let left = clip_mut(value, clip_id)?;
            left.end_frame = *at_frame;
            left.fade_out_frames = 0;
            left.fade_in_frames = left.fade_in_frames.min(left.duration_frames());
            lane_mut(value, &lane_id)?.clips.push(right);
            let effects: Vec<_> = value
                .eq
                .iter()
                .filter(|effect| {
                    effect.scope
                        == (EqScope::Clip {
                            clip_id: clip_id.clone(),
                        })
                })
                .cloned()
                .collect();
            for mut effect in effects {
                effect.id = derived_effect_id(value, &effect.id, new_clip_id);
                effect.scope = EqScope::Clip {
                    clip_id: new_clip_id.clone(),
                };
                value.eq.push(effect);
            }
            retain_effects(value);
        }
        Edit::DuplicateClip {
            clip_id,
            new_clip_id,
            lane_id,
            start_frame,
        } => {
            let mut clip = value.clip(clip_id)?.clone();
            clip.id = new_clip_id.clone();
            clip.end_frame = start_frame
                .checked_add(clip.duration_frames())
                .context("Duplicate coordinates overflow")?;
            clip.start_frame = *start_frame;
            lane_mut(value, lane_id)?.clips.push(clip);
            let effects: Vec<_> = value
                .eq
                .iter()
                .filter(|effect| {
                    effect.scope
                        == (EqScope::Clip {
                            clip_id: clip_id.clone(),
                        })
                })
                .cloned()
                .collect();
            for mut effect in effects {
                effect.id = derived_effect_id(value, &effect.id, new_clip_id);
                effect.scope = EqScope::Clip {
                    clip_id: new_clip_id.clone(),
                };
                value.eq.push(effect);
            }
        }
        Edit::DeleteClip { clip_id } => {
            remove_clip(value, clip_id)?;
            retain_effects(value);
        }
        Edit::SetClipGain { clip_id, gain_db } => clip_mut(value, clip_id)?.gain_db = *gain_db,
        Edit::SetClipName { clip_id, name } => clip_mut(value, clip_id)?.name = name.clone(),
        Edit::SetClipFades {
            clip_id,
            fade_in_frames,
            fade_out_frames,
        } => {
            let clip = clip_mut(value, clip_id)?;
            clip.fade_in_frames = *fade_in_frames;
            clip.fade_out_frames = *fade_out_frames;
        }
        Edit::SetEq { effect } => {
            if let Some(existing) = value.eq.iter_mut().find(|old| old.id == effect.id) {
                *existing = effect.clone();
            } else {
                value.eq.push(effect.clone());
            }
        }
        Edit::RemoveEq { eq_id } => {
            let index = value
                .eq
                .iter()
                .position(|effect| &effect.id == eq_id)
                .context("Scoped EQ ID is not present")?;
            value.eq.remove(index);
        }
    }
    Ok(())
}

fn derived_effect_id(value: &Arrangement, effect_id: &str, clip_id: &str) -> String {
    // Stable identity derivation only, not a security/content hash.
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let hash = effect_id
        .bytes()
        .chain(std::iter::once(0))
        .chain(clip_id.bytes())
        .fold(OFFSET, |hash, byte| {
            (hash ^ byte as u64).wrapping_mul(PRIME)
        });
    for suffix in 0_u64.. {
        let candidate = format!("eq_{hash:016x}_{suffix}");
        if !value.eq.iter().any(|item| item.id == candidate)
            && !value.lanes.iter().any(|lane| {
                lane.id == candidate || lane.clips.iter().any(|clip| clip.id == candidate)
            })
        {
            return candidate;
        }
    }
    unreachable!("The bounded metadata budget cannot exhaust 64-bit ID suffixes")
}
