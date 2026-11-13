use anyhow::{Result, ensure};
use mastercraft::{
    audio::Audio,
    config::Settings,
    meters::{self, Metrics},
    wav_encoding,
};
use std::io::Cursor;

pub fn encode_checked(
    mut source: Audio,
    format: &str,
    settings: &Settings,
) -> Result<(Vec<u8>, Audio, Metrics, usize)> {
    ensure!(
        ["wav16", "wav24", "wav32f"].contains(&format),
        "Choose a WAV export format"
    );
    for pass in 1..=5 {
        let sample_bytes = match format {
            "wav16" => 2,
            "wav24" => 3,
            _ => 4,
        };
        // Hound writes progressively. Reserve the known payload and header so
        // long exports never grow a second, rounded-up full-song allocation.
        let capacity = source
            .samples
            .len()
            .checked_mul(sample_bytes)
            .and_then(|size| size.checked_add(128))
            .ok_or_else(|| anyhow::anyhow!("WAV export is too large"))?;
        let mut cursor = Cursor::new(Vec::with_capacity(capacity));
        wav_encoding::write_wav(&source, &mut cursor, format, settings.dither)?;
        let bytes = cursor.into_inner();
        let mut reader = hound::WavReader::new(Cursor::new(&bytes))?;
        let spec = reader.spec();
        let mut samples = Vec::with_capacity(source.samples.len());
        if spec.sample_format == hound::SampleFormat::Float {
            for sample in reader.samples::<f32>() {
                samples.push(sample?);
            }
        } else {
            let scale = 2_f64.powi(spec.bits_per_sample as i32 - 1);
            for sample in reader.samples::<i32>() {
                samples.push((sample? as f64 / scale) as f32);
            }
        }
        let decoded = Audio::new(samples, spec.sample_rate, spec.channels as usize)?;
        let metrics = meters::measure(&decoded)?;
        if metrics.true_peak_dbtp <= settings.ceiling_dbtp + 0.005 {
            return Ok((bytes, decoded, metrics, pass));
        }
        source.gain(settings.ceiling_dbtp - 0.15 - metrics.true_peak_dbtp);
    }
    anyhow::bail!("Encoded WAV still exceeds the selected peak ceiling")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_export_preserves_deterministic_bytes_and_decoded_frames() {
        let source = Audio::new(
            (0..96000)
                .map(|i| ((i * 37 % 101) as f32 - 50.) / 500.)
                .collect(),
            48000,
            2,
        )
        .unwrap();
        let settings = Settings::default();
        for format in ["wav16", "wav24", "wav32f"] {
            let mut baseline = Cursor::new(Vec::new());
            wav_encoding::write_wav(&source, &mut baseline, format, settings.dither).unwrap();
            let (bytes, decoded, metrics, passes) =
                encode_checked(source.clone(), format, &settings).unwrap();
            assert_eq!(bytes, baseline.into_inner());
            assert_eq!(decoded.samples.len(), source.samples.len());
            assert_eq!(decoded.sample_rate, source.sample_rate);
            assert_eq!(decoded.channels, source.channels);
            assert!(decoded.samples.iter().all(|v| v.is_finite()));
            assert!(metrics.true_peak_dbtp <= settings.ceiling_dbtp + 0.005);
            assert_eq!(passes, 1);
        }
    }
}
