use crate::{
    config::Settings,
    dsp::biquad::{self, Biquad},
    meters::lin,
};

pub type Frame = [f64; 2];
const IDENTITY: Biquad = Biquad {
    b: [1., 0., 0.],
    a: [1., 0., 0.],
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EqParameters {
    pub coefficients: [Biquad; 7],
    pub enabled: [bool; 7],
    pub gain_db: f64,
    pub ceiling_dbtp: f64,
    pub post_gain_db: f64,
}
impl EqParameters {
    pub fn new(settings: &Settings, sample_rate: u32, hold_gain_db: f64) -> Self {
        let mut coefficients = [IDENTITY; 7];
        let mut enabled = [false; 7];
        for (i, band) in settings.manual_eq.iter().take(7).enumerate() {
            enabled[i] = band.on && (band.gain.abs() > 1e-9 || band.kind.ends_with("cut"));
            if enabled[i] {
                coefficients[i] = biquad::from_band(band, sample_rate);
            }
        }
        Self {
            coefficients,
            enabled,
            gain_db: (hold_gain_db + settings.eq_output_db).clamp(-30., 30.),
            ceiling_dbtp: settings.ceiling_dbtp,
            post_gain_db: 0.,
        }
    }
    pub fn active(&self) -> bool {
        self.enabled.iter().any(|v| *v) || self.gain_db.abs() > 1e-9
    }
    pub fn filters_equal(&self, other: &Self) -> bool {
        self.coefficients == other.coefficients && self.enabled == other.enabled
    }
    pub fn decay_seconds(&self, sr: u32) -> f64 {
        self.coefficients
            .iter()
            .zip(self.enabled)
            .filter(|(_, on)| *on)
            .map(|(c, _)| {
                let d = c.a[1] * c.a[1] - 4. * c.a[2];
                let radius = if d < 0. {
                    c.a[2].abs().sqrt()
                } else {
                    ((-c.a[1] + d.sqrt()) * 0.5)
                        .abs()
                        .max(((-c.a[1] - d.sqrt()) * 0.5).abs())
                };
                if radius > 0. && radius < 1. {
                    -1. / (sr as f64 * radius.ln())
                } else {
                    0.
                }
            })
            .fold(0_f64, f64::max)
    }
}

#[derive(Clone, Copy)]
pub struct EqChain {
    pub parameters: EqParameters,
    state: [[[f64; 2]; 2]; 7],
    gain: f64,
}
impl EqChain {
    pub fn new(parameters: EqParameters) -> Self {
        Self {
            parameters,
            state: [[[0.; 2]; 2]; 7],
            gain: lin(parameters.gain_db),
        }
    }
    pub fn gain_change(&self, parameters: EqParameters) -> Self {
        Self {
            parameters,
            gain: lin(parameters.gain_db),
            state: self.state,
        }
    }
    #[inline]
    pub fn process(&mut self, input: Frame, channels: usize) -> Frame {
        let mut output = [0.; 2];
        for ch in 0..channels {
            let mut value = input[ch];
            for band in 0..7 {
                if !self.parameters.enabled[band] {
                    continue;
                }
                let c = &self.parameters.coefficients[band];
                let z = &mut self.state[band][ch];
                let next = c.b[0] * value + z[0];
                z[0] = c.b[1] * value - c.a[1] * next + z[1];
                z[1] = c.b[2] * value - c.a[2] * next;
                value = next;
            }
            output[ch] = value * self.gain;
        }
        output
    }
}
