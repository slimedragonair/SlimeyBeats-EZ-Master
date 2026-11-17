//! Optional rate conformance of a session derivative. The original file is never changed.
use crate::{
    audio::{Audio, external_command},
    native_process, wav_encoding,
};
use anyhow::{Context, Result, ensure};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub fn conform(
    input: Audio,
    rate: u32,
    max_bytes: usize,
    working_directory: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<Audio> {
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    ensure!(
        (8000..=384_000).contains(&rate),
        "Unsupported project sample rate"
    );
    ensure!(
        (8000..=384_000).contains(&input.sample_rate)
            && (1..=2).contains(&input.channels)
            && !input.samples.is_empty()
            && input.samples.len().is_multiple_of(input.channels),
        "Rate conformance requires valid mono or stereo PCM"
    );
    ensure!(
        input
            .samples
            .len()
            .checked_mul(4)
            .is_some_and(|bytes| bytes <= max_bytes),
        "Audio exceeds the decoded-source budget"
    );
    if input.sample_rate == rate {
        return Ok(input);
    }
    let expected_frames =
        (input.frames() as u128 * rate as u128).div_ceil(input.sample_rate as u128);
    ensure!(
        expected_frames * input.channels as u128 * 4 <= max_bytes as u128,
        "Conformed audio exceeds the remaining decoded-source budget"
    );
    let source = tempfile::Builder::new()
        .prefix("arrangement-conform-")
        .suffix(".wav")
        .tempfile_in(working_directory)?;
    wav_encoding::write_wav(&input, source.as_file(), "wav32f", false)?;
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    let mut command = external_command("ffmpeg");
    command
        .args([
            "-v",
            "error",
            "-nostdin",
            "-protocol_whitelist",
            "file,pipe",
            "-f",
            "wav",
            "-i",
        ])
        .arg(source.path())
        .args([
            "-ar",
            &rate.to_string(),
            "-ac",
            &input.channels.to_string(),
            "-c:a",
            "pcm_f32le",
            "-f",
            "f32le",
            "pipe:1",
        ]);
    let channels = input.channels;
    let mut samples = Vec::new();
    let mut pending = Vec::new();
    native_process::read_stdout(
        command,
        max_bytes,
        Duration::from_secs(300),
        cancel.clone(),
        |bytes| {
            pending.extend_from_slice(bytes);
            let complete = pending.len() / 4 * 4;
            samples
                .try_reserve(complete / 4)
                .context("Cannot allocate conformed audio")?;
            for bytes in pending[..complete].as_chunks::<4>().0 {
                let value = f32::from_le_bytes(*bytes);
                ensure!(value.is_finite(), "Rate conformance produced nonfinite PCM");
                samples.push(value);
            }
            let remaining = pending.len() - complete;
            pending.copy_within(complete.., 0);
            pending.truncate(remaining);
            Ok(())
        },
    )
    .context("Cannot conform audio to the project rate; FFmpeg is required")?;
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    ensure!(
        pending.is_empty(),
        "Rate conformance returned incomplete PCM"
    );
    Audio::new(samples, rate, channels)
}
