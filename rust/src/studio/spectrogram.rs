use super::stft::Transform;
use crate::audio::Audio;
use anyhow::{Result, ensure};
use serde::Serialize;

const FFT_SIZE: usize = 2048;
#[derive(Clone, Serialize)]
pub struct Spectrogram {
    pub columns: usize,
    pub rows: usize,
    pub duration_seconds: f64,
    pub frequencies_hz: Vec<f64>,
    pub levels_dbfs: Vec<f32>,
}
/// A bounded sampled STFT display. Values are mean channel bin power in dBFS,
/// not integrated octave energy or an EQ response.
pub fn build(
    audio: &Audio,
    columns: usize,
    rows: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Spectrogram> {
    ensure!(
        (16..=1024).contains(&columns) && (16..=256).contains(&rows),
        "Spectrogram resolution must be 16–1024 columns and 16–256 rows"
    );
    let high = audio.sample_rate as f64 / 2.;
    let frequencies_hz = (0..rows)
        .map(|i| 20. * (high / 20.).powf(i as f64 / (rows - 1) as f64))
        .collect::<Vec<_>>();
    let mut levels_dbfs = vec![-120.; columns * rows];
    let mut plan = Transform::new(FFT_SIZE);
    let norm = plan.window.iter().sum::<f32>().powi(2);
    for col in 0..columns {
        ensure!(!cancel(), "Cancelled");
        let center = (col as f64 / (columns - 1) as f64 * audio.frames().saturating_sub(1) as f64)
            .round() as isize;
        let mut power = vec![0_f32; rows];
        for ch in 0..audio.channels {
            let bins = plan.at(audio, ch, center);
            for (row, &freq) in frequencies_hz.iter().enumerate() {
                let index = (freq * FFT_SIZE as f64 / audio.sample_rate as f64).round() as usize;
                let index = index.min(FFT_SIZE / 2);
                let one_sided = if index == 0 || index == FFT_SIZE / 2 {
                    1.
                } else {
                    2.
                };
                power[row] += one_sided * bins[index].norm_sqr() / norm / audio.channels as f32;
            }
        }
        for row in 0..rows {
            levels_dbfs[col * rows + row] = 10. * power[row].max(1e-12).log10();
        }
    }
    Ok(Spectrogram {
        columns,
        rows,
        duration_seconds: audio.duration(),
        frequencies_hz,
        levels_dbfs,
    })
}
