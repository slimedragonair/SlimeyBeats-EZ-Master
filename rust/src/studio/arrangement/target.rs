//! One scope/coordinate extraction path for native, WASM, MCP and server analysis.
use super::{
    Arrangement, ArrangementPlayer, AssetLibrary, EqScope, FrameRange, Lane, RenderOptions,
};
use crate::audio::Audio;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetBasis {
    pub scope: EqScope,
    pub range: FrameRange,
    pub coordinate_domain: String,
    pub excluded_eq_id: Option<String>,
    pub isolated: bool,
    pub extended_source_context: bool,
}
pub struct TargetAudio {
    pub audio: Audio,
    pub offset_frame: u64,
    pub basis: TargetBasis,
}

pub fn render_eq_target(
    document: &Arrangement,
    assets: &AssetLibrary,
    scope: &EqScope,
    range: Option<FrameRange>,
    exclude_eq_id: Option<&str>,
    options: &RenderOptions,
    cancel: &dyn Fn() -> bool,
) -> Result<TargetAudio> {
    ensure!(!cancel(), "Cancelled");
    document.validate(assets)?;
    options.validate()?;
    let mut isolated = document.clone();
    if let Some(id) = exclude_eq_id {
        let effect = isolated
            .eq
            .iter()
            .find(|effect| effect.id == id)
            .context("Excluded EQ identity is unavailable")?;
        ensure!(
            effect.scope == *scope && effect.range == range,
            "Excluded EQ must match the requested target scope and exact range"
        );
        isolated.eq.retain(|effect| effect.id != id);
    }
    let (selected, project_range, extended) = match scope {
        EqScope::Mix {} => {
            let selected = range.unwrap_or(FrameRange {
                start_frame: 0,
                end_frame: document.duration_frames(),
            });
            bounds(
                selected,
                FrameRange {
                    start_frame: 0,
                    end_frame: document.duration_frames(),
                },
            )?;
            (selected, selected, false)
        }
        EqScope::Lane { lane_id } => {
            let lane = document.lane(lane_id)?;
            let first = lane
                .clips
                .iter()
                .map(|clip| clip.start_frame)
                .min()
                .context("Selected track has no clips")?;
            let end = lane.clips.iter().map(|clip| clip.end_frame).max().unwrap();
            let selected = range.unwrap_or(FrameRange {
                start_frame: first,
                end_frame: end,
            });
            bounds(
                selected,
                FrameRange {
                    start_frame: 0,
                    end_frame: document.duration_frames(),
                },
            )?;
            for lane in &mut isolated.lanes {
                lane.mute = &lane.id != lane_id;
                lane.solo = false;
            }
            (selected, selected, false)
        }
        EqScope::Clip { clip_id } => {
            let clip = document.clip(clip_id)?.clone();
            let selected = range.unwrap_or(FrameRange {
                start_frame: clip.source_start_frame,
                end_frame: clip.source_end_frame()?,
            });
            bounds(
                selected,
                FrameRange {
                    start_frame: 0,
                    end_frame: assets[&clip.source_id].frames() as u64,
                },
            )?;
            let selected_lane = document
                .lanes
                .iter()
                .find(|lane| lane.clips.iter().any(|candidate| candidate.id == *clip_id))
                .unwrap()
                .id
                .clone();
            let mut removed = Vec::new();
            for lane in &mut isolated.lanes {
                lane.solo = false;
                lane.mute = lane.id != selected_lane;
                if !lane.mute {
                    let clips = std::mem::take(&mut lane.clips);
                    for candidate in clips {
                        if candidate.id == *clip_id {
                            lane.clips.push(candidate);
                        } else {
                            removed.push(candidate);
                        }
                    }
                }
            }
            if !removed.is_empty() {
                let mut silent = Lane::new(
                    unique_lane_id(document),
                    "Other clips excluded from analysis",
                );
                silent.mute = true;
                silent.clips = removed;
                isolated.lanes.push(silent);
            }
            let visible_end = clip.source_end_frame()?;
            let extended =
                selected.start_frame < clip.source_start_frame || selected.end_frame > visible_end;
            let source_start = selected.start_frame.min(clip.source_start_frame);
            let source_end = selected.end_frame.max(visible_end);
            let leading = clip.source_start_frame - source_start;
            let shift = leading.saturating_sub(clip.start_frame);
            if shift > 0 {
                shift_project(&mut isolated, shift)?;
            }
            let selected_clip = isolated
                .lanes
                .iter_mut()
                .flat_map(|lane| &mut lane.clips)
                .find(|candidate| candidate.id == *clip_id)
                .unwrap();
            selected_clip.start_frame = clip
                .start_frame
                .checked_add(shift)
                .and_then(|frame| frame.checked_sub(leading))
                .context("Target coordinate overflow")?;
            selected_clip.source_start_frame = source_start;
            selected_clip.end_frame = selected_clip
                .start_frame
                .checked_add(source_end - source_start)
                .context("Target coordinate overflow")?;
            let first = selected_clip
                .start_frame
                .checked_add(selected.start_frame - source_start)
                .context("Target coordinate overflow")?;
            (
                selected,
                FrameRange {
                    start_frame: first,
                    end_frame: first
                        .checked_add(selected.end_frame - selected.start_frame)
                        .context("Target coordinate overflow")?,
                },
                extended,
            )
        }
    };
    let count = (project_range.end_frame - project_range.start_frame)
        .checked_mul(document.channels as u64)
        .context("Target output size overflow")?;
    ensure!(
        count
            .checked_mul(4)
            .is_some_and(|bytes| bytes <= options.max_output_bytes),
        "Selected EQ target exceeds the decoded output budget"
    );
    isolated.validate(assets)?;
    let mut player = ArrangementPlayer::new_cancellable(&isolated, assets, options, cancel)?;
    if project_range.start_frame > 0 {
        player.seek(project_range.start_frame, cancel)?;
    }
    let mut pcm = Vec::new();
    pcm.try_reserve_exact(
        usize::try_from(count).context("Target output does not fit this platform")?,
    )?;
    while player.position() < project_range.end_frame {
        if player.position().is_multiple_of(4096) {
            ensure!(!cancel(), "Cancelled");
        }
        let frame = player
            .next_frame()?
            .context("Target range exceeds arranged audio")?;
        pcm.extend_from_slice(&frame[..document.channels]);
    }
    ensure!(!cancel(), "Cancelled");
    Ok(TargetAudio {
        audio: Audio::new(pcm, document.sample_rate, document.channels)?,
        offset_frame: selected.start_frame,
        basis: TargetBasis {
            scope: scope.clone(),
            range: selected,
            coordinate_domain: if matches!(scope, EqScope::Clip { .. }) {
                "source"
            } else {
                "project"
            }
            .into(),
            excluded_eq_id: exclude_eq_id.map(str::to_owned),
            isolated: !matches!(scope, EqScope::Mix {}),
            extended_source_context: extended,
        },
    })
}
fn bounds(selected: FrameRange, available: FrameRange) -> Result<()> {
    ensure!(
        selected.start_frame < selected.end_frame
            && selected.start_frame >= available.start_frame
            && selected.end_frame <= available.end_frame,
        "EQ target frame bounds are outside the selected scope"
    );
    Ok(())
}
fn unique_lane_id(document: &Arrangement) -> String {
    (0_u64..)
        .map(|index| format!("target-muted-{index}"))
        .find(|id| {
            document
                .lanes
                .iter()
                .all(|lane| lane.id != *id && lane.clips.iter().all(|clip| clip.id != *id))
                && document.eq.iter().all(|effect| effect.id != *id)
        })
        .expect("Finite metadata leaves an available identity")
}
fn shift_project(document: &mut Arrangement, shift: u64) -> Result<()> {
    for clip in document.lanes.iter_mut().flat_map(|lane| &mut lane.clips) {
        clip.start_frame = clip
            .start_frame
            .checked_add(shift)
            .context("Target project coordinate overflow")?;
        clip.end_frame = clip
            .end_frame
            .checked_add(shift)
            .context("Target project coordinate overflow")?;
    }
    for effect in &mut document.eq {
        if !matches!(effect.scope, EqScope::Clip { .. })
            && let Some(range) = &mut effect.range
        {
            range.start_frame = range
                .start_frame
                .checked_add(shift)
                .context("Target effect coordinate overflow")?;
            range.end_frame = range
                .end_frame
                .checked_add(shift)
                .context("Target effect coordinate overflow")?;
        }
    }
    Ok(())
}
