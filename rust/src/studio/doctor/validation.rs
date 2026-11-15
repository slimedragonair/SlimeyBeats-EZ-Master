use super::thresholds::CANCEL_FRAMES;
use crate::audio::Audio;
use anyhow::{Context, Result, ensure};

pub(super) fn layout(audio: &Audio) -> Result<()> {
    ensure!(
        (8000..=384000).contains(&audio.sample_rate),
        "Sample rate must be 8000–384000 Hz"
    );
    ensure!(
        (1..=2).contains(&audio.channels),
        "Doctor supports mono or stereo audio"
    );
    ensure!(
        !audio.samples.is_empty() && audio.samples.len().is_multiple_of(audio.channels),
        "Empty or incomplete audio frames"
    );
    Ok(())
}

pub(super) fn samples(audio: &Audio, cancel: &dyn Fn() -> bool) -> Result<()> {
    for chunk in audio.samples.chunks(CANCEL_FRAMES * audio.channels) {
        ensure!(!cancel(), "Cancelled");
        ensure!(
            chunk.iter().all(|v| v.is_finite()),
            "Audio contains NaN or infinite samples"
        );
    }
    ensure!(!cancel(), "Cancelled");
    Ok(())
}

pub(super) fn copy(audio: &Audio, cancel: &dyn Fn() -> bool) -> Result<Audio> {
    ensure!(!cancel(), "Cancelled");
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(audio.samples.len())
        .context("Cannot allocate doctor revision")?;
    for chunk in audio.samples.chunks(CANCEL_FRAMES * audio.channels) {
        ensure!(!cancel(), "Cancelled");
        ensure!(
            chunk.iter().all(|v| v.is_finite()),
            "Audio contains NaN or infinite samples"
        );
        samples.extend_from_slice(chunk);
    }
    ensure!(!cancel(), "Cancelled");
    Ok(Audio {
        samples,
        sample_rate: audio.sample_rate,
        channels: audio.channels,
    })
}
