use super::request::Limits;
use anyhow::{Result, ensure};
use mastercraft::{audio::Audio, studio_native::audio_import};
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

pub fn load(path: &Path, limits: &Limits, budget: usize) -> Result<Audio> {
    let budget = budget.min(limits.max_pcm_bytes);
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
    {
        let reader = hound::WavReader::open(path)?;
        let rate = reader.spec().sample_rate;
        ensure!(
            rate > 0 && reader.duration() as f64 / rate as f64 <= limits.max_duration_seconds,
            "Input exceeds configured duration budget"
        );
        ensure!(
            reader.len() as u64 * 4 <= budget as u64,
            "Input exceeds configured decoded PCM budget"
        );
    }
    let (audio, _) = audio_import::load(
        path,
        budget,
        Duration::from_secs(limits.decode_timeout_seconds),
        Arc::new(AtomicBool::new(false)),
    )?;
    validate(&audio, limits, budget)?;
    Ok(audio)
}

pub fn validate(audio: &Audio, limits: &Limits, budget: usize) -> Result<()> {
    ensure!(
        (1..=2).contains(&audio.channels) && (8000..=384000).contains(&audio.sample_rate),
        "Invalid audio dimensions"
    );
    ensure!(
        !audio.samples.is_empty() && audio.samples.len().is_multiple_of(audio.channels),
        "Invalid audio frames"
    );
    ensure!(
        audio.samples.len() <= budget / 4 && audio.duration() <= limits.max_duration_seconds,
        "Audio exceeds configured PCM or duration budget"
    );
    ensure!(
        audio.samples.iter().all(|sample| sample.is_finite()),
        "Non-finite PCM"
    );
    Ok(())
}
