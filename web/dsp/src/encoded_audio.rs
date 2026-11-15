//! Measure decoded saved audio separately from the editable preview.
use crate::{failure, import_limits};
use anyhow::{Result, ensure};
use mastercraft::{analysis, audio::Audio};
use wasm_bindgen::prelude::*;

// MP3 frames can add a short encoder/decoder tail. This output-only allowance
// never changes the strict input duration cap or the decoded memory budget.
const MAX_CODEC_TAIL_SECONDS: f64 = 0.25;

fn measure_checked(samples: Vec<f32>, rate: u32, channels: u32, profile: &str) -> Result<String> {
    ensure!((8000..=192000).contains(&rate), "Invalid saved-audio rate");
    ensure!(
        (1..=2).contains(&channels),
        "Use mono or stereo saved audio"
    );
    ensure!(
        samples.len() <= import_limits::MAX_DECODED_BYTES / size_of::<f32>(),
        "Saved-audio verification exceeds the browser memory budget"
    );
    let duration = samples.len() as f64 / channels as f64 / rate as f64;
    ensure!(
        duration <= import_limits::MAX_SECONDS + MAX_CODEC_TAIL_SECONDS,
        "Saved audio is longer than the browser export limit"
    );
    let audio = Audio::new(samples, rate, channels as usize)?;
    let analysis = analysis::analyze(&audio, &crate::profile(profile)?)?;
    Ok(serde_json::json!({
        "metrics": analysis.metrics, "analysis": analysis, "frames": audio.frames(),
        "sample_rate": rate, "channels": channels, "duration_seconds": duration
    })
    .to_string())
}

#[wasm_bindgen]
pub fn measure_encoded_audio(
    samples: Vec<f32>,
    rate: u32,
    channels: u32,
    profile: &str,
) -> Result<String, JsValue> {
    measure_checked(samples, rate, channels, profile).map_err(failure)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_saved_samples_and_layout_without_a_session() {
        let samples: Vec<_> = (0..48000).map(|i| (i as f32 * 0.1).sin() * 0.25).collect();
        let value: serde_json::Value =
            serde_json::from_str(&measure_checked(samples, 48000, 1, "neutral").unwrap()).unwrap();
        assert_eq!(value["frames"], 48000);
        assert_eq!(value["channels"], 1);
        assert_eq!(value["duration_seconds"], 1.);
        assert_eq!(value["metrics"]["clipped_samples"], 0);
        assert!(value["metrics"]["true_peak_dbtp"].as_f64().unwrap() < -11.);
    }

    #[test]
    fn rejects_invalid_saved_audio_before_measurement() {
        for (samples, rate, channels) in [
            (vec![0.; 20], 0, 1),
            (vec![0.; 20], 48000, 3),
            (vec![0.; 3], 48000, 2),
            (vec![f32::NAN; 20], 48000, 1),
            (vec![0.; 7_202_001], 8000, 1),
        ] {
            assert!(measure_checked(samples, rate, channels, "neutral").is_err());
        }
    }
}
