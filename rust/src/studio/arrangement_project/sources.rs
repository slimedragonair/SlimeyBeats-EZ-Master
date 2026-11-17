use super::ProjectSource;
use crate::{audio::Audio, source_identity::SourceIdentity};
use anyhow::{Result, ensure};
use std::io::Cursor;

pub fn encode_source(audio: &Audio) -> Result<Vec<u8>> {
    let mut output = Cursor::new(Vec::new());
    crate::wav_encoding::write_wav(audio, &mut output, "wav32f", false)?;
    Ok(output.into_inner())
}
pub fn describe_source(
    id: String,
    name: String,
    audio: &Audio,
    bytes: &[u8],
) -> Result<ProjectSource> {
    let identity = SourceIdentity::for_bytes(bytes);
    let value = ProjectSource {
        id,
        name,
        file: format!("assets/{}.wav", identity.sha256),
        identity,
        frames: audio.frames() as u64,
        sample_rate: audio.sample_rate,
        channels: audio.channels,
    };
    value.validate()?;
    Ok(value)
}
pub fn decode_source(
    source: &ProjectSource,
    bytes: &[u8],
    max_decoded_bytes: u64,
) -> Result<Audio> {
    source.validate()?;
    ensure!(
        SourceIdentity::for_bytes(bytes) == source.identity,
        "Project source hash/size mismatch"
    );
    let mut reader = hound::WavReader::new(Cursor::new(bytes))?;
    let spec = reader.spec();
    ensure!(
        spec.sample_format == hound::SampleFormat::Float && spec.bits_per_sample == 32,
        "Arrangement source pool requires immutable Float32 WAV"
    );
    ensure!(
        reader.duration() as u64 == source.frames
            && spec.sample_rate == source.sample_rate
            && spec.channels as usize == source.channels,
        "Project WAV geometry differs from its manifest"
    );
    ensure!(
        (reader.len() as u64)
            .checked_mul(4)
            .is_some_and(|bytes| bytes <= max_decoded_bytes),
        "Project source exceeds the decoded memory budget"
    );
    let samples = reader
        .samples::<f32>()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Audio::new(samples, spec.sample_rate, spec.channels as usize)
}
