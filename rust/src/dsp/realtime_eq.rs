use super::{
    eq_chain::{EqChain, EqParameters, Frame},
    stream_peak_guard::StreamPeakGuard,
};
use crate::meters::lin;
use std::f64::consts::PI;

const MIN_WARM_SECONDS: f64 = 0.170;
const MAX_WARM_SECONDS: f64 = 1.5;

pub struct RealtimeEq {
    current: EqChain,
    next: Option<EqChain>,
    pending: Option<EqParameters>,
    history: Vec<Frame>,
    written: usize,
    filled: usize,
    fade_position: usize,
    fade_length: usize,
    sample_rate: u32,
    channels: usize,
    guard: StreamPeakGuard,
    post_gain: f64,
    post_target: f64,
}
impl RealtimeEq {
    pub fn reset(&mut self, parameters: EqParameters) {
        self.current = EqChain::new(parameters);
        self.next = None;
        self.pending = None;
        self.written = 0;
        self.filled = 0;
        self.fade_position = 0;
        self.post_gain = lin(parameters.post_gain_db.min(0.));
        self.post_target = self.post_gain;
        self.guard.reset(parameters.ceiling_dbtp);
    }
    pub fn new(sr: u32, channels: usize, parameters: EqParameters) -> Self {
        Self {
            current: EqChain::new(parameters),
            next: None,
            pending: None,
            history: vec![[0.; 2]; (MAX_WARM_SECONDS * sr as f64).ceil() as usize],
            written: 0,
            filled: 0,
            fade_position: 0,
            fade_length: 1,
            sample_rate: sr,
            channels,
            guard: StreamPeakGuard::new(sr, parameters.ceiling_dbtp),
            post_gain: lin(parameters.post_gain_db.min(0.)),
            post_target: lin(parameters.post_gain_db.min(0.)),
        }
    }
    pub fn delay(&self) -> usize {
        self.guard.delay
    }
    pub fn parameters(&self) -> EqParameters {
        self.pending
            .or_else(|| self.next.map(|c| c.parameters))
            .unwrap_or(self.current.parameters)
    }
    pub fn set_parameters(&mut self, parameters: EqParameters) {
        if self.parameters() != parameters {
            if self.parameters().ceiling_dbtp != parameters.ceiling_dbtp {
                self.guard.set_ceiling(parameters.ceiling_dbtp);
            }
            self.post_target = lin(parameters.post_gain_db.min(0.));
            self.pending = Some(parameters);
        }
    }
    pub fn warm_frames(&self) -> usize {
        ((8. * self.parameters().decay_seconds(self.sample_rate))
            .clamp(MIN_WARM_SECONDS, MAX_WARM_SECONDS)
            * self.sample_rate as f64)
            .ceil() as usize
    }
    fn start_transition(&mut self) {
        if self.next.is_some() {
            return;
        }
        let Some(parameters) = self.pending.take() else {
            return;
        };
        if parameters == self.current.parameters {
            return;
        }
        let same = parameters.filters_equal(&self.current.parameters);
        let mut next = if same {
            self.current.gain_change(parameters)
        } else {
            EqChain::new(parameters)
        };
        let tau = parameters
            .decay_seconds(self.sample_rate)
            .max(self.current.parameters.decay_seconds(self.sample_rate));
        if !same {
            let count = ((8. * tau).clamp(MIN_WARM_SECONDS, MAX_WARM_SECONDS)
                * self.sample_rate as f64)
                .ceil() as usize;
            let count = count.min(self.filled);
            for i in 0..count {
                let slot = (self.written + self.history.len() - count + i) % self.history.len();
                next.process(self.history[slot], self.channels);
            }
        }
        let seconds = if same {
            0.080
        } else {
            (1.5 * tau).clamp(0.020, 0.300)
        };
        self.fade_length = (seconds * self.sample_rate as f64).ceil().max(1.) as usize;
        self.fade_position = 0;
        self.next = Some(next);
    }
    #[inline]
    pub fn process(&mut self, input: Frame) -> Frame {
        self.start_transition();
        let a = self.current.process(input, self.channels);
        let mut active = self.current.parameters.active();
        let output = if let Some(next) = &mut self.next {
            let b = next.process(input, self.channels);
            active |= next.parameters.active();
            let weight =
                0.5 - 0.5 * (PI * (self.fade_position + 1) as f64 / self.fade_length as f64).cos();
            self.fade_position += 1;
            let mixed = [a[0] + (b[0] - a[0]) * weight, a[1] + (b[1] - a[1]) * weight];
            if self.fade_position >= self.fade_length {
                self.current = self.next.take().unwrap();
            }
            mixed
        } else {
            a
        };
        self.history[self.written] = input;
        self.written = (self.written + 1) % self.history.len();
        self.filled = (self.filled + 1).min(self.history.len());
        let guarded = self.guard.process(output, self.channels, active);
        self.post_gain += (self.post_target - self.post_gain).clamp(
            -1. / (0.080 * self.sample_rate as f64),
            1. / (0.080 * self.sample_rate as f64),
        );
        [guarded[0] * self.post_gain, guarded[1] * self.post_gain]
    }
}
