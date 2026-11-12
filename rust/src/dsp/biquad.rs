use crate::{audio::Audio, config::EqBand, meters::lin};
use std::f64::consts::{PI, SQRT_2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    pub b: [f64; 3],
    pub a: [f64; 3],
}
impl Biquad {
    fn normalized(b: [f64; 3], a: [f64; 3]) -> Self {
        Self {
            b: b.map(|x| x / a[0]),
            a: a.map(|x| x / a[0]),
        }
    }
    pub fn design(kind: &str, freq: f64, gain: f64, q: f64, sr: u32) -> Self {
        let w = 2. * PI * freq.clamp(5., sr as f64 * 0.49) / sr as f64;
        let (s, c) = w.sin_cos();
        let alpha = s / (2. * q.clamp(0.1, 18.));
        let a = lin(gain / 2.);
        match kind {
            "low_cut" => Self::normalized(
                [(1. + c) / 2., -(1. + c), (1. + c) / 2.],
                [1. + alpha, -2. * c, 1. - alpha],
            ),
            "high_cut" => Self::normalized(
                [(1. - c) / 2., 1. - c, (1. - c) / 2.],
                [1. + alpha, -2. * c, 1. - alpha],
            ),
            "low_shelf" => {
                let v = 2. * a.sqrt() * alpha;
                Self::normalized(
                    [
                        a * ((a + 1.) - (a - 1.) * c + v),
                        2. * a * ((a - 1.) - (a + 1.) * c),
                        a * ((a + 1.) - (a - 1.) * c - v),
                    ],
                    [
                        (a + 1.) + (a - 1.) * c + v,
                        -2. * ((a - 1.) + (a + 1.) * c),
                        (a + 1.) + (a - 1.) * c - v,
                    ],
                )
            }
            "high_shelf" => {
                let v = 2. * a.sqrt() * alpha;
                Self::normalized(
                    [
                        a * ((a + 1.) + (a - 1.) * c + v),
                        -2. * a * ((a - 1.) + (a + 1.) * c),
                        a * ((a + 1.) + (a - 1.) * c - v),
                    ],
                    [
                        (a + 1.) - (a - 1.) * c + v,
                        2. * ((a - 1.) - (a + 1.) * c),
                        (a + 1.) - (a - 1.) * c - v,
                    ],
                )
            }
            _ => Self::normalized(
                [1. + alpha * a, -2. * c, 1. - alpha * a],
                [1. + alpha / a, -2. * c, 1. - alpha / a],
            ),
        }
    }
    pub fn apply(&self, audio: &mut Audio) {
        for channel in 0..audio.channels {
            let (mut z1, mut z2) = (0., 0.);
            for i in (channel..audio.samples.len()).step_by(audio.channels) {
                let x = audio.samples[i] as f64;
                let y = self.b[0] * x + z1;
                z1 = self.b[1] * x - self.a[1] * y + z2;
                z2 = self.b[2] * x - self.a[2] * y;
                audio.samples[i] = y as f32;
            }
        }
    }
    pub fn response_db(&self, frequency: f64, sr: u32) -> f64 {
        let w = 2. * PI * frequency / sr as f64;
        let power = |x: [f64; 3]| {
            (x[0] + x[1] * w.cos() + x[2] * (2. * w).cos()).powi(2)
                + (x[1] * w.sin() + x[2] * (2. * w).sin()).powi(2)
        };
        10. * (power(self.b) / power(self.a).max(1e-30))
            .max(1e-30)
            .log10()
    }
}
pub fn from_band(band: &EqBand, sr: u32) -> Biquad {
    Biquad::design(&band.kind, band.freq, band.gain, band.q, sr)
}
pub fn highpass(audio: &mut Audio, frequency: f64) {
    Biquad::design("low_cut", frequency, 0., 1. / SQRT_2, audio.sample_rate).apply(audio);
}
