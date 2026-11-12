use super::{eq_chain::Frame, oversampling::bessel};
use crate::meters::lin;
use std::{collections::VecDeque, f64::consts::PI};

const TAPS: usize = 32;
const MARGIN_DB: f64 = 0.2;

pub struct StreamPeakGuard {
    history: [[f64; TAPS]; 2],
    fir: [[f64; TAPS]; 3],
    ring: Vec<Frame>,
    queue: VecDeque<(usize, f64)>,
    average: Vec<f64>,
    sum: f64,
    average_position: usize,
    index: usize,
    length: usize,
    pub delay: usize,
    ceiling: f64,
    release: f64,
    gain: f64,
}
impl StreamPeakGuard {
    pub fn reset(&mut self, db: f64) {
        self.history = [[0.; TAPS]; 2];
        self.ring.fill([0.; 2]);
        self.queue.clear();
        self.average.fill(1.);
        self.sum = self.length as f64;
        self.average_position = 0;
        self.index = 0;
        self.gain = 1.;
        self.ceiling = lin(db - MARGIN_DB);
    }
    pub fn set_ceiling(&mut self, db: f64) {
        let ceiling = lin(db - MARGIN_DB);
        let ratio = ceiling / self.ceiling;
        self.ceiling = ceiling;
        for (_, gain) in &mut self.queue {
            *gain = (*gain * ratio).min(1.);
        }
        for gain in &mut self.average {
            *gain = (*gain * ratio).min(1.);
        }
        self.sum = self.average.iter().sum();
        self.gain = (self.gain * ratio).min(1.);
    }
    pub fn new(sr: u32, ceiling_dbtp: f64) -> Self {
        let length = (sr as f64 * 0.002).round().max(8.) as usize;
        let delay = length + TAPS / 2 - 1;
        let mut fir = [[0.; TAPS]; 3];
        for (phase, taps) in fir.iter_mut().enumerate() {
            let fraction = (phase + 1) as f64 / 4.;
            for (i, tap) in taps.iter_mut().enumerate() {
                let t = i as f64 - (TAPS / 2 - 1) as f64 - fraction;
                let x = 2. * i as f64 / (TAPS - 1) as f64 - 1.;
                *tap = if t.abs() < 1e-12 {
                    1.
                } else {
                    (PI * t).sin() / (PI * t)
                } * bessel(8. * (1. - x * x).max(0.).sqrt())
                    / bessel(8.);
            }
            let sum = taps.iter().sum::<f64>();
            for tap in taps {
                *tap /= sum;
            }
        }
        Self {
            history: [[0.; TAPS]; 2],
            fir,
            ring: vec![[0.; 2]; (delay + 2).next_power_of_two()],
            queue: VecDeque::with_capacity(length + 2),
            average: vec![1.; length],
            sum: length as f64,
            average_position: 0,
            index: 0,
            length,
            delay,
            ceiling: lin(ceiling_dbtp - MARGIN_DB),
            release: 1. - (-1. / (0.060 * sr as f64)).exp(),
            gain: 1.,
        }
    }
    #[inline]
    pub fn process(&mut self, input: Frame, channels: usize, active: bool) -> Frame {
        let mut peak = 0_f64;
        for (ch, &sample) in input.iter().enumerate().take(channels) {
            let history = &mut self.history[ch];
            history.copy_within(1..TAPS, 0);
            history[TAPS - 1] = sample;
            if active {
                peak = peak.max(history[TAPS / 2 - 1].abs());
                for phase in &self.fir {
                    let mut value = 0.;
                    for i in 0..TAPS {
                        value += phase[i] * history[i];
                    }
                    peak = peak.max(value.abs());
                }
            }
        }
        let required = if peak > self.ceiling {
            self.ceiling / peak
        } else {
            1.
        };
        while self.queue.back().is_some_and(|p| p.1 >= required) {
            self.queue.pop_back();
        }
        self.queue.push_back((self.index, required));
        while self
            .queue
            .front()
            .is_some_and(|p| p.0 + self.length <= self.index)
        {
            self.queue.pop_front();
        }
        let minimum = self.queue.front().map(|p| p.1).unwrap_or(1.);
        self.sum += minimum - self.average[self.average_position];
        self.average[self.average_position] = minimum;
        self.average_position = (self.average_position + 1) % self.length;
        let target = (self.sum / self.length as f64).clamp(0., 1.);
        if target < self.gain {
            self.gain = target;
        } else {
            self.gain += (target - self.gain) * self.release;
        }
        let slot = self.index % self.ring.len();
        self.ring[slot] = input;
        let output = if self.index >= self.delay {
            let frame = self.ring[(self.index - self.delay) % self.ring.len()];
            [frame[0] * self.gain, frame[1] * self.gain]
        } else {
            [0.; 2]
        };
        self.index += 1;
        output
    }
}
