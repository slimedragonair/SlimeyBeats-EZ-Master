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
        let mut cursor = Cursor::new(Vec::new());
        wav_encoding::write_wav(&source, &mut cursor, format, settings.dither)?;
        let bytes = cursor.into_inner();
        let mut reader = hound::WavReader::new(Cursor::new(&bytes))?;
        let spec = reader.spec();
        let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
            reader
                .samples::<f32>()
                .collect::<std::result::Result<_, _>>()?
        } else {
            let scale = 2_f64.powi(spec.bits_per_sample as i32 - 1);
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| (v as f64 / scale) as f32))
                .collect::<std::result::Result<_, _>>()?
        };
        let decoded = Audio::new(samples, spec.sample_rate, spec.channels as usize)?;
        let metrics = meters::measure(&decoded)?;
        if metrics.true_peak_dbtp <= settings.ceiling_dbtp + 0.005 {
            return Ok((bytes, decoded, metrics, pass));
        }
        source.gain(settings.ceiling_dbtp - 0.15 - metrics.true_peak_dbtp);
    }
    anyhow::bail!("Encoded WAV still exceeds the selected peak ceiling")
}
