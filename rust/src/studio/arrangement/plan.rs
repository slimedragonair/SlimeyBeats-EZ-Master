use super::{
    Arrangement, AssetLibrary, Clip, EqScope,
    eq_processor::{EqProcessor, process_chain},
    gain::{GainRamp, lane_levels},
};
use crate::{audio::Audio, dsp::eq_chain::Frame, meters::lin};
use anyhow::{Result, ensure};
use std::{collections::BTreeSet, sync::Arc};

pub(super) struct ClipVoice {
    pub meta: Clip,
    pub source: Arc<Audio>,
    pub lane: usize,
    pub effects: Vec<EqProcessor>,
    pub gain: GainRamp,
}
impl ClipVoice {
    pub fn input(&self, frame: u64, channels: usize) -> Frame {
        let index = frame as usize * self.source.channels;
        let left = self.source.samples[index] as f64;
        [
            left,
            if channels == 2 {
                self.source.samples[index + usize::from(self.source.channels == 2)] as f64
            } else {
                0.
            },
        ]
    }
    pub fn process(&mut self, project_frame: u64, channels: usize) -> Frame {
        let source_frame = self.meta.source_start_frame + project_frame - self.meta.start_frame;
        let dry = self.input(source_frame, channels);
        let mut frame = process_chain(&mut self.effects, dry, source_frame);
        let position = project_frame - self.meta.start_frame;
        let duration = self.meta.duration_frames();
        let fade = |position: u64, frames: u64| {
            if frames == 0 {
                1.
            } else if frames == 1 {
                0.
            } else {
                (position as f64 / (frames - 1) as f64).clamp(0., 1.)
            }
        };
        let envelope = fade(position, self.meta.fade_in_frames)
            * fade(duration - 1 - position, self.meta.fade_out_frames);
        let gain = self.gain.next();
        for channel in 0..channels {
            frame[channel] *= gain[channel] * envelope;
        }
        frame
    }
}
pub(super) struct LaneVoice {
    pub effects: Vec<EqProcessor>,
    pub gain: GainRamp,
}
#[derive(Clone, Copy)]
pub(super) struct Event {
    pub frame: u64,
    pub start: bool,
    pub clip: usize,
}
pub(super) struct Plan {
    pub clips: Vec<ClipVoice>,
    pub lanes: Vec<LaneVoice>,
    pub mix_effects: Vec<EqProcessor>,
    pub events: Vec<Event>,
}
impl Plan {
    pub fn new(
        document: &Arrangement,
        assets: &AssetLibrary,
        cancel: &dyn Fn() -> bool,
    ) -> Result<Self> {
        Self::compile(document, assets, cancel, true)
    }
    pub fn compile(
        document: &Arrangement,
        assets: &AssetLibrary,
        cancel: &dyn Fn() -> bool,
        scan_pcm: bool,
    ) -> Result<Self> {
        Self::compile_at(document, assets, cancel, scan_pcm, 0, None)
    }
    pub fn compile_at(
        document: &Arrangement,
        assets: &AssetLibrary,
        cancel: &dyn Fn() -> bool,
        scan_pcm: bool,
        project_start: u64,
        source_warm_limit: Option<u64>,
    ) -> Result<Self> {
        document.validate(assets)?;
        let mut scanned = BTreeSet::new();
        for clip in document.lanes.iter().flat_map(|lane| &lane.clips) {
            if scan_pcm && scanned.insert(&clip.source_id) {
                for chunk in assets[&clip.source_id].samples.chunks(16_384) {
                    ensure!(!cancel(), "Cancelled");
                    ensure!(
                        chunk.iter().all(|sample| sample.is_finite()),
                        "Arrangement source contains nonfinite PCM"
                    );
                }
            }
        }
        let mut result = Self {
            clips: vec![],
            lanes: vec![],
            mix_effects: vec![],
            events: vec![],
        };
        let any_solo = document.lanes.iter().any(|lane| lane.solo);
        for (lane_index, lane) in document.lanes.iter().enumerate() {
            let effects = document
                .eq
                .iter()
                .filter(|effect| {
                    effect.scope
                        == (EqScope::Lane {
                            lane_id: lane.id.clone(),
                        })
                })
                .map(|effect| EqProcessor::new(effect, document.sample_rate, document.channels))
                .collect();
            result.lanes.push(LaneVoice {
                effects,
                gain: GainRamp::new(lane_levels(lane, any_solo), document.sample_rate),
            });
            for clip in &lane.clips {
                ensure!(!cancel(), "Cancelled");
                let effects = document
                    .eq
                    .iter()
                    .filter(|effect| {
                        effect.scope
                            == (EqScope::Clip {
                                clip_id: clip.id.clone(),
                            })
                    })
                    .map(|effect| EqProcessor::new(effect, document.sample_rate, document.channels))
                    .collect();
                let mut voice = ClipVoice {
                    meta: clip.clone(),
                    source: assets[&clip.source_id].clone(),
                    lane: lane_index,
                    effects,
                    gain: GainRamp::new([lin(clip.gain_db); 2], document.sample_rate),
                };
                // Source-coordinate EQ has a canonical source-zero history, so
                // trimming/splitting a clip retains its intended filter state.
                if !voice.effects.is_empty() && clip.end_frame > project_start {
                    let first_source =
                        clip.source_start_frame + project_start.saturating_sub(clip.start_frame);
                    let warm_start =
                        source_warm_limit.map_or(0, |limit| first_source.saturating_sub(limit));
                    for frame in warm_start..first_source {
                        if frame.is_multiple_of(4096) {
                            ensure!(!cancel(), "Cancelled");
                        }
                        let input = voice.input(frame, document.channels);
                        process_chain(&mut voice.effects, input, frame);
                    }
                }
                let index = result.clips.len();
                result.events.push(Event {
                    frame: clip.start_frame,
                    start: true,
                    clip: index,
                });
                result.events.push(Event {
                    frame: clip.end_frame,
                    start: false,
                    clip: index,
                });
                result.clips.push(voice);
            }
        }
        result
            .events
            .sort_by_key(|event| (event.frame, event.start, event.clip));
        result.mix_effects = document
            .eq
            .iter()
            .filter(|effect| matches!(effect.scope, EqScope::Mix {}))
            .map(|effect| EqProcessor::new(effect, document.sample_rate, document.channels))
            .collect();
        Ok(result)
    }
}
