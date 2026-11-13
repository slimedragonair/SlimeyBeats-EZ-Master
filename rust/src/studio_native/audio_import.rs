use crate::native_process as codec_process;
use crate::{
    audio::{Audio, external_command},
    source_identity::SourceIdentity,
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Deserialize)]
struct Probe {
    streams: Vec<Stream>,
}
#[derive(Deserialize)]
struct Stream {
    sample_rate: String,
    channels: usize,
}

pub fn load(
    path: &Path,
    max_bytes: usize,
    timeout: Duration,
    cancel: Arc<AtomicBool>,
) -> Result<(Audio, SourceIdentity)> {
    ensure!(
        path.metadata()?.len() <= (max_bytes as u64).saturating_mul(4),
        "Encoded file exceeds the configured input budget"
    );
    let cancelled = || cancel.load(Ordering::Relaxed);
    let identity = SourceIdentity::read(path, &cancelled)?;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let audio = if extension == "wav" {
        let mut reader = hound::WavReader::open(path)?;
        ensure!(
            (1..=2).contains(&reader.spec().channels),
            "Use mono or stereo audio"
        );
        ensure!(
            reader.len() as usize <= max_bytes / 4,
            "Decoded WAV exceeds the configured memory budget"
        );
        let spec = reader.spec();
        let expected = reader.len() as usize;
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(expected)
            .context("Not enough memory for decoded audio")?;
        let mut push = |sample: Result<f32>| -> Result<()> {
            if samples.len() % 16384 == 0 {
                ensure!(!cancelled(), "Cancelled");
            }
            ensure!(
                samples.len() < max_bytes / 4,
                "Decoded WAV exceeds the configured memory budget"
            );
            samples.push(sample?);
            Ok(())
        };
        match spec.sample_format {
            hound::SampleFormat::Float => {
                for sample in reader.samples::<f32>() {
                    push(sample.map_err(Into::into))?;
                }
            }
            hound::SampleFormat::Int => {
                let scale = 2_f64.powi(spec.bits_per_sample as i32 - 1);
                for sample in reader.samples::<i32>() {
                    push(
                        sample
                            .map(|v| (v as f64 / scale) as f32)
                            .map_err(Into::into),
                    )?;
                }
            }
        }
        ensure!(samples.len() == expected, "Truncated WAV");
        Audio::new(samples, spec.sample_rate, spec.channels as usize)?
    } else {
        // Force the expected self-contained media demuxer: a renamed playlist
        // must not turn an approved audio path into network or unrelated-file access.
        let demuxer = match extension.as_str() {
            "mp3" => "mp3",
            "flac" => "flac",
            "ogg" | "oga" | "opus" => "ogg",
            "m4a" => "mov",
            "aac" => "aac",
            "aiff" | "aif" => "aiff",
            "wma" => "asf",
            _ => anyhow::bail!("Choose a supported audio format"),
        };
        let mut probe = external_command("ffprobe");
        probe
            .args([
                "-v",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-f",
                demuxer,
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=sample_rate,channels",
                "-of",
                "json",
            ])
            .arg(path);
        let mut bytes = Vec::new();
        codec_process::read_stdout(probe, 65536, timeout, cancel.clone(), |chunk| {
            bytes.extend_from_slice(chunk);
            Ok(())
        })?;
        let probe: Probe =
            serde_json::from_slice(&bytes).context("Invalid codec probe response")?;
        let stream = probe.streams.first().context("No audio stream")?;
        ensure!(
            (1..=2).contains(&stream.channels),
            "Use mono or stereo audio"
        );
        let rate: u32 = stream.sample_rate.parse()?;
        ensure!((8000..=384000).contains(&rate), "Unsupported sample rate");
        let mut command = external_command("ffmpeg");
        command
            .args([
                "-v",
                "error",
                "-nostdin",
                "-protocol_whitelist",
                "file,pipe",
                "-f",
                demuxer,
                "-i",
            ])
            .arg(path)
            .args([
                "-map",
                "0:a:0",
                "-vn",
                "-sn",
                "-dn",
                "-f",
                "f32le",
                "-acodec",
                "pcm_f32le",
                "pipe:1",
            ]);
        let mut samples = Vec::new();
        let mut pending = Vec::with_capacity(65540);
        codec_process::read_stdout(command, max_bytes, timeout, cancel.clone(), |chunk| {
            pending.extend_from_slice(chunk);
            let complete = pending.len() / 4 * 4;
            samples
                .try_reserve(complete / 4)
                .context("Not enough memory for decoded audio")?;
            for bytes in pending[..complete].as_chunks::<4>().0 {
                samples.push(f32::from_le_bytes(*bytes));
            }
            pending.drain(..complete);
            Ok(())
        })?;
        ensure!(pending.is_empty(), "Truncated decoded audio");
        Audio::new(samples, rate, stream.channels)?
    };
    ensure!(!cancelled(), "Cancelled");
    ensure!(
        SourceIdentity::read(path, &cancelled)? == identity,
        "Input changed while it was being decoded; import it again"
    );
    Ok((audio, identity))
}
