use crate::{analysis::Spectrum, config::Settings, dsp::biquad, meters::lin};

/// An immediate estimate while dragging; a full EBU measurement resolves it after editing settles.
pub struct EqLoudnessModel {
    frequencies: Vec<f64>,
    power: Vec<f64>,
    total: f64,
    sample_rate: u32,
    silent: bool,
}
impl EqLoudnessModel {
    pub fn new(spectrum: &Spectrum, sample_rate: u32, input_lufs: f64) -> Self {
        let shelf = biquad::Biquad::design(
            "high_shelf",
            1681.97445,
            3.99984385,
            0.70717524,
            sample_rate,
        );
        let hp = biquad::Biquad::design("low_cut", 38.13547, 0., 0.50032704, sample_rate);
        let power: Vec<_> = spectrum
            .freqs
            .iter()
            .zip(&spectrum.db)
            .map(|(&frequency, &db)| {
                let weight = shelf.response_db(frequency, sample_rate)
                    + hp.response_db(frequency, sample_rate);
                // Display points are equally spaced in log frequency, so represented bandwidth is proportional to f.
                10_f64.powf((db + weight) / 10.) * frequency
            })
            .collect();
        let total = power.iter().sum();
        Self {
            frequencies: spectrum.freqs.clone(),
            power,
            total,
            sample_rate,
            silent: input_lufs <= -69.5,
        }
    }
    pub fn delta_db(&self, settings: &Settings) -> f64 {
        if self.silent || self.total <= 1e-20 {
            return 0.;
        }
        let filters: Vec<_> = settings
            .manual_eq
            .iter()
            .filter(|b| b.on)
            .map(|b| biquad::from_band(b, self.sample_rate))
            .collect();
        let output = self
            .frequencies
            .iter()
            .zip(&self.power)
            .map(|(&frequency, &power)| {
                let gain = filters
                    .iter()
                    .map(|filter| filter.response_db(frequency, self.sample_rate))
                    .sum::<f64>();
                power * lin(gain).powi(2)
            })
            .sum::<f64>();
        (10. * (output / self.total).max(1e-20).log10()).clamp(-18., 18.)
    }
    pub fn hold_gain(&self, settings: &Settings) -> f64 {
        if settings.eq_hold_loudness {
            -self.delta_db(settings)
        } else {
            0.
        }
    }
}
