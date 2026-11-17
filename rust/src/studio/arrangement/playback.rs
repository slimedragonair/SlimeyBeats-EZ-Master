#[path = "playback_controls.rs"]
mod controls;
#[path = "playback_prepare.rs"]
mod preparation;
use super::{
    Arrangement, AssetLibrary, EqScope, RenderOptions, ScopedEq,
    eq_processor::{CONTROL_FADE_SECONDS, process_chain},
    gain::{GainRamp, lane_levels},
    plan::Plan,
};
use crate::{
    dsp::{eq_chain::Frame, stream_peak_guard::StreamPeakGuard},
    meters::lin,
};
use anyhow::{Context, Result, ensure};

/// Prepared off the audio thread. Playback visits boundary events and active
/// clips, not every clip in the document for every emitted sample.
pub struct ArrangementPlayer {
    document: Arrangement,
    assets: AssetLibrary,
    options: RenderOptions,
    plan: Plan,
    active: Vec<usize>,
    event: usize,
    input_position: u64,
    position: u64,
    duration: u64,
    lane_frames: Vec<Frame>,
    lane_filled: Vec<bool>,
    output_gain: GainRamp,
    guard: StreamPeakGuard,
    previous: Option<Box<Self>>,
    pending: Option<Box<Self>>,
    retired: Option<Box<Self>>,
    fade_position: usize,
    fade_frames: usize,
}
impl ArrangementPlayer {
    pub fn new(
        document: &Arrangement,
        assets: &AssetLibrary,
        options: &RenderOptions,
    ) -> Result<Self> {
        Self::new_cancellable(document, assets, options, &|| false)
    }
    pub fn new_cancellable(
        document: &Arrangement,
        assets: &AssetLibrary,
        options: &RenderOptions,
        cancel: &dyn Fn() -> bool,
    ) -> Result<Self> {
        ensure!(!cancel(), "Cancelled");
        options.validate()?;
        let plan = Plan::new(document, assets, cancel)?;
        let mut prepared = Self::new_from_plan(document, assets, options, plan);
        prepared.prime_lookahead(cancel)?;
        Ok(prepared)
    }
    fn new_from_plan(
        document: &Arrangement,
        assets: &AssetLibrary,
        options: &RenderOptions,
        plan: Plan,
    ) -> Self {
        let lanes = plan.lanes.len();
        let active = Vec::with_capacity(plan.clips.len());
        Self {
            document: document.clone(),
            assets: assets.clone(),
            options: options.clone(),
            plan,
            active,
            event: 0,
            input_position: 0,
            position: 0,
            duration: document.duration_frames(),
            lane_frames: vec![[0.; 2]; lanes],
            lane_filled: vec![false; lanes],
            output_gain: GainRamp::new([lin(options.output_gain_db); 2], document.sample_rate),
            guard: StreamPeakGuard::new(document.sample_rate, options.ceiling_dbtp),
            previous: None,
            pending: None,
            retired: None,
            fade_position: 0,
            fade_frames: (CONTROL_FADE_SECONDS * document.sample_rate as f64)
                .round()
                .max(1.) as usize,
        }
    }
    fn prime_lookahead(&mut self, cancel: &dyn Fn() -> bool) -> Result<()> {
        let end = self.position + self.guard.delay as u64;
        while self.input_position < end {
            ensure!(!cancel(), "Cancelled");
            let input = self.mixed_input_frame(self.input_position);
            ensure!(
                input[..self.channels()]
                    .iter()
                    .all(|value| value.is_finite()),
                "Arrangement processing overflowed"
            );
            self.guard
                .process(input, self.channels(), self.options.protect_peaks);
            self.input_position += 1;
        }
        Ok(())
    }
    pub fn sample_rate(&self) -> u32 {
        self.document.sample_rate
    }
    pub fn channels(&self) -> usize {
        self.document.channels
    }
    pub fn duration_frames(&self) -> u64 {
        self.duration
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    pub fn next_frame(&mut self) -> Result<Option<[f32; 2]>> {
        if self.position >= self.duration_frames() {
            return Ok(None);
        }
        let wanted = self.position + self.guard.delay as u64;
        let mut frame = [0.; 2];
        while self.input_position <= wanted {
            let input = self.mixed_input_frame(self.input_position);
            ensure!(
                input[..self.channels()]
                    .iter()
                    .all(|value| value.is_finite()),
                "Arrangement processing overflowed"
            );
            frame = self
                .guard
                .process(input, self.channels(), self.options.protect_peaks);
            self.input_position += 1;
        }
        let output = frame.map(|sample| sample as f32);
        self.position += 1;
        if let Some(pending) = &mut self.pending {
            pending.position = self.position;
        }
        if self.previous.is_some() && self.fade_position >= self.fade_frames {
            self.retired = self.previous.take();
        }
        ensure!(
            output[..self.channels()]
                .iter()
                .all(|sample| sample.is_finite()),
            "Arrangement output overflowed float PCM"
        );
        Ok(Some(output))
    }
    fn mixed_input_frame(&mut self, frame: u64) -> Frame {
        let mut output = self.input_frame(frame);
        if let Some(previous) = &mut self.previous {
            let old = previous.input_frame(frame);
            self.fade_position += 1;
            let fraction = (self.fade_position as f64 / self.fade_frames as f64).min(1.);
            let weight = 0.5 - 0.5 * (std::f64::consts::PI * fraction).cos();
            for channel in 0..self.document.channels {
                output[channel] = old[channel] * (1. - weight) + output[channel] * weight;
            }
        }
        if let Some(pending) = &mut self.pending {
            pending.input_frame(frame);
            pending.input_position = frame + 1;
        }
        output
    }
    fn input_frame(&mut self, frame: u64) -> Frame {
        while let Some(event) = self.plan.events.get(self.event) {
            if event.frame > frame {
                break;
            }
            if event.start {
                let index = self.active.binary_search(&event.clip).unwrap_err();
                self.active.insert(index, event.clip);
            } else if let Ok(index) = self.active.binary_search(&event.clip) {
                self.active.remove(index);
            }
            self.event += 1;
        }
        self.lane_frames.fill([0.; 2]);
        self.lane_filled.fill(false);
        for &index in &self.active {
            let clip = &mut self.plan.clips[index];
            let value = clip.process(frame, self.document.channels);
            let lane = clip.lane;
            if self.lane_filled[lane] {
                for (channel, value) in value.iter().enumerate().take(self.document.channels) {
                    self.lane_frames[lane][channel] += *value;
                }
            } else {
                self.lane_frames[lane] = value;
                self.lane_filled[lane] = true;
            }
        }
        let mut mixed = [0.; 2];
        let mut filled = false;
        for (index, lane) in self.plan.lanes.iter_mut().enumerate() {
            let mut value = process_chain(&mut lane.effects, self.lane_frames[index], frame);
            let gain = lane.gain.next();
            for channel in 0..self.document.channels {
                value[channel] *= gain[channel];
            }
            if self.lane_filled[index] || value.iter().any(|sample| *sample != 0.) {
                if filled {
                    for channel in 0..self.document.channels {
                        mixed[channel] += value[channel];
                    }
                } else {
                    mixed = value;
                    filled = true;
                }
            }
        }
        mixed = process_chain(&mut self.plan.mix_effects, mixed, frame);
        let gain = self.output_gain.next();
        for channel in 0..self.document.channels {
            mixed[channel] *= gain[channel];
        }
        mixed
    }
    pub fn process_block(&mut self, output: &mut [f32]) -> Result<usize> {
        let channels = self.channels();
        ensure!(
            output.len().is_multiple_of(channels),
            "Arrangement block has incomplete channel frames"
        );
        let mut written = 0;
        for destination in output.chunks_exact_mut(channels) {
            if let Some(frame) = self.next_frame()? {
                destination.copy_from_slice(&frame[..channels]);
                written += 1;
            } else {
                destination.fill(0.);
            }
        }
        Ok(written)
    }
    pub fn replace_prepared(&mut self, replacement: Self) -> Result<()> {
        let retired = self.replace_prepared_boxed(Box::new(replacement))?;
        // This convenience method belongs on a control thread; callback users
        // use the boxed handoff and return retired engines to a disposal queue.
        drop(retired);
        Ok(())
    }
    pub fn replace_prepared_boxed(
        &mut self,
        mut replacement: Box<Self>,
    ) -> Result<Option<Box<Self>>> {
        ensure!(
            replacement.sample_rate() == self.sample_rate()
                && replacement.channels() == self.channels(),
            "Prepared replacement layout differs"
        );
        ensure!(
            replacement.position() == self.position(),
            "Prepared replacement must be primed at the current project frame"
        );
        ensure!(
            replacement.previous.is_none()
                && replacement.pending.is_none()
                && replacement.retired.is_none(),
            "Prepare a fresh arrangement replacement"
        );
        let retired = self.retired.take();
        if self.previous.is_some() {
            return Ok(self.pending.replace(replacement));
        } else {
            std::mem::swap(self, replacement.as_mut());
            // Blend project engines before one continuous final peak guard.
            std::mem::swap(&mut self.guard, &mut replacement.guard);
            self.guard.set_ceiling(self.options.ceiling_dbtp);
            self.previous = Some(replacement);
            self.fade_position = 0;
        }
        Ok(retired)
    }
    /// Transfer retired storage out of the callback. Queued prepared replacements
    /// begin only once the retired slot is drained, keeping ownership bounded.
    pub fn take_retired(&mut self) -> Result<Option<Box<Self>>> {
        let retired = self.retired.take();
        if self.previous.is_none()
            && let Some(next) = self.pending.take()
        {
            self.replace_prepared_boxed(next)?;
        }
        Ok(retired)
    }
}
