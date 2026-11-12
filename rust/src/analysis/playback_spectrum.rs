use crate::audio::Audio;
use rustfft::{Fft, FftPlanner, num_complex::Complex32};
use std::{f64::consts::PI, sync::Arc};

const FFT_SIZE: usize = 4096;
const POINTS: usize = 160;

pub struct PlaybackSpectrum {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    buffer: Vec<Complex32>,
    scratch: Vec<Complex32>,
    pub frequencies: Vec<f64>,
    pub before_db: Vec<f64>,
    pub after_db: Vec<f64>,
    position: Option<usize>,
    rate: u32,
}
impl Default for PlaybackSpectrum {
    fn default() -> Self {
        let fft = FftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
        let scratch = vec![Complex32::default(); fft.get_inplace_scratch_len()];
        Self {
            fft,
            window: (0..FFT_SIZE)
                .map(|i| (0.5 - 0.5 * (2. * PI * i as f64 / FFT_SIZE as f64).cos()) as f32)
                .collect(),
            buffer: vec![Complex32::default(); FFT_SIZE],
            scratch,
            frequencies: vec![],
            before_db: vec![],
            after_db: vec![],
            position: None,
            rate: 0,
        }
    }
}
impl PlaybackSpectrum {
    pub fn reset(&mut self) {
        self.position = None;
    }
    pub fn update(&mut self, before: &Audio, after: Option<&Audio>, seconds: f64) {
        let frame = ((seconds.max(0.) * before.sample_rate as f64) as usize)
            .min(before.frames().saturating_sub(1));
        let bucket = frame / 256;
        if self.position == Some(bucket) && self.rate == before.sample_rate {
            return;
        }
        self.position = Some(bucket);
        self.rate = before.sample_rate;
        let top = 20000_f64.min(before.sample_rate as f64 * 0.49);
        self.frequencies = (0..POINTS)
            .map(|i| 20. * (top / 20.).powf(i as f64 / (POINTS - 1) as f64))
            .collect();
        self.before_db = self.measure(before, frame.saturating_sub(FFT_SIZE / 2));
        self.after_db = after
            .map(|audio| self.measure(audio, frame.saturating_sub(FFT_SIZE / 2)))
            .unwrap_or_default();
    }
    pub fn update_live(
        &mut self,
        before: &Audio,
        after: Option<&Audio>,
        seconds: f64,
        tap: Option<&Audio>,
    ) {
        self.update(before, after, seconds);
        if let Some(tap) = tap {
            self.after_db = self.measure(tap, tap.frames().saturating_sub(FFT_SIZE / 2));
        }
    }
    fn measure(&mut self, audio: &Audio, frame: usize) -> Vec<f64> {
        let mut power = vec![0_f64; FFT_SIZE / 2 + 1];
        for ch in 0..audio.channels {
            for i in 0..FFT_SIZE {
                let index = frame as isize + i as isize - (FFT_SIZE / 2) as isize;
                let value = if index >= 0 && (index as usize) < audio.frames() {
                    audio.samples[index as usize * audio.channels + ch]
                } else {
                    0.
                };
                self.buffer[i] = Complex32::new(value * self.window[i], 0.);
            }
            self.fft
                .process_with_scratch(&mut self.buffer, &mut self.scratch);
            for (p, value) in power.iter_mut().zip(&self.buffer) {
                *p += value.norm_sqr() as f64;
            }
        }
        let scale = 2.
            / (FFT_SIZE as f64
                * self.window.iter().map(|&w| (w as f64).powi(2)).sum::<f64>()
                * audio.channels as f64);
        let bin_hz = audio.sample_rate as f64 / FFT_SIZE as f64;
        self.frequencies
            .iter()
            .map(|&f| {
                let lo = ((f * 2_f64.powf(-1. / 12.) / bin_hz) as usize).min(power.len() - 1);
                let hi =
                    (((f * 2_f64.powf(1. / 12.) / bin_hz) as usize) + 1).clamp(lo + 1, power.len());
                let energy = power[lo..hi].iter().sum::<f64>() * scale / (hi - lo) as f64;
                10. * energy.max(1e-12).log10()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn analyzer_tracks_actual_rendered_gain_and_silence() {
        let samples = (0..48000)
            .map(|i| (0.1 * (2. * PI * 1000. * i as f64 / 48000.).sin()) as f32)
            .collect();
        let before = Audio::new(samples, 48000, 1).unwrap();
        let mut after = before.clone();
        after.gain(6.);
        let mut meter = PlaybackSpectrum::default();
        meter.update(&before, Some(&after), 0.5);
        let index = meter
            .before_db
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        assert!((meter.after_db[index] - meter.before_db[index] - 6.).abs() < 0.01);
        let zero = Audio::new(vec![0.; 48000], 48000, 1).unwrap();
        meter.reset();
        meter.update(&zero, None, 0.5);
        assert!(meter.before_db.iter().all(|x| x.is_finite() && *x <= -100.));
    }
}
