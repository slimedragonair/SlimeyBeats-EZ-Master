use super::{FrameRange, ScopedEq};
use crate::{
    config::Settings,
    dsp::eq_chain::{EqChain, EqParameters, Frame},
};

pub(super) const CONTROL_FADE_SECONDS: f64 = 0.020;
const RANGE_FADE_SECONDS: f64 = 0.005;

fn parameters(spec: &ScopedEq, rate: u32) -> EqParameters {
    let mut settings = Settings {
        manual_eq: spec.bands.clone(),
        eq_output_db: spec.output_gain_db,
        eq_hold_loudness: false,
        ..Settings::default()
    };
    if spec.bypass {
        for band in &mut settings.manual_eq {
            band.on = false;
        }
        settings.eq_output_db = 0.;
    }
    EqParameters::new(&settings, rate, 0.)
}

/// Wet filters run continuously, including outside an enabled range. There is
/// deliberately no voice limiter: one final guard owns project peak protection.
pub(super) struct EqProcessor {
    // Target/range stay fixed; current editable metadata belongs to the document.
    pub spec: ScopedEq,
    current: EqChain,
    next: Option<EqChain>,
    pending: Option<EqParameters>,
    transition: usize,
    fade_frames: usize,
    range_fade_frames: u64,
    rate: u32,
    channels: usize,
}
pub struct PreparedEqUpdate {
    pub(super) sample_rate: u32,
    pub(super) spec: ScopedEq,
    pub(super) document_spec: Option<ScopedEq>,
    pub(super) parameters: EqParameters,
}
impl PreparedEqUpdate {
    pub fn new(spec: &ScopedEq, sample_rate: u32) -> anyhow::Result<Self> {
        spec.validate_settings()?;
        anyhow::ensure!(
            (8000..=384_000).contains(&sample_rate),
            "Invalid scoped EQ sample rate"
        );
        Ok(Self {
            sample_rate,
            spec: spec.clone(),
            document_spec: Some(spec.clone()),
            parameters: parameters(spec, sample_rate),
        })
    }
    pub fn id(&self) -> &str {
        &self.spec.id
    }
    pub fn matches_target(&self, effect: &ScopedEq) -> bool {
        self.spec.id == effect.id
            && self.spec.scope == effect.scope
            && self.spec.range == effect.range
    }
}
impl EqProcessor {
    pub fn new(spec: &ScopedEq, rate: u32, channels: usize) -> Self {
        Self {
            spec: spec.clone(),
            current: EqChain::new(parameters(spec, rate)),
            next: None,
            pending: None,
            transition: 0,
            fade_frames: (CONTROL_FADE_SECONDS * rate as f64).round().max(1.) as usize,
            range_fade_frames: (RANGE_FADE_SECONDS * rate as f64).round().max(1.) as u64,
            rate,
            channels,
        }
    }
    pub fn update(&mut self, spec: &ScopedEq) {
        self.update_prepared(parameters(spec, self.rate));
    }
    pub fn apply_prepared(&mut self, update: &PreparedEqUpdate) {
        self.update_prepared(update.parameters);
    }
    fn update_prepared(&mut self, next: EqParameters) {
        if self.next.is_some() {
            self.pending = Some(next);
            return;
        }
        if self.current.parameters == next {
            return;
        }
        self.next = Some(if next.filters_equal(&self.current.parameters) {
            self.current.gain_change(next)
        } else {
            EqChain::new(next)
        });
        self.transition = 0;
    }
    pub fn process(&mut self, input: Frame, coordinate: u64) -> Frame {
        if self.next.is_none() && !self.current.parameters.active() {
            return input;
        }
        let mut wet = self.current.process(input, self.channels);
        if let Some(next) = &mut self.next {
            let other = next.process(input, self.channels);
            self.transition += 1;
            let weight = raised_cosine(self.transition as f64 / self.fade_frames as f64);
            for channel in 0..self.channels {
                wet[channel] = wet[channel] * (1. - weight) + other[channel] * weight;
            }
            if self.transition >= self.fade_frames {
                self.current = self.next.take().unwrap();
                if let Some(spec) = self.pending.take() {
                    self.update_prepared(spec);
                }
            }
        }
        let weight = self.spec.range.map_or(1., |range| {
            range_weight(range, coordinate, self.range_fade_frames)
        });
        if weight == 0. {
            input
        } else if weight == 1. {
            wet
        } else {
            let mut output = input;
            for channel in 0..self.channels {
                output[channel] = input[channel] * (1. - weight) + wet[channel] * weight;
            }
            output
        }
    }
}
fn raised_cosine(fraction: f64) -> f64 {
    0.5 - 0.5 * (std::f64::consts::PI * fraction.clamp(0., 1.)).cos()
}
fn range_weight(range: FrameRange, frame: u64, fade: u64) -> f64 {
    if frame < range.start_frame || frame >= range.end_frame {
        return 0.;
    }
    let length = range.end_frame - range.start_frame;
    let fade = fade.min(length.saturating_sub(1) / 2);
    if fade == 0 {
        return 0.;
    }
    let distance = (frame - range.start_frame).min(range.end_frame - 1 - frame);
    raised_cosine(distance as f64 / fade as f64)
}

pub(super) fn process_chain(
    effects: &mut [EqProcessor],
    mut frame: Frame,
    coordinate: u64,
) -> Frame {
    for effect in effects {
        frame = effect.process(frame, coordinate);
    }
    frame
}
