//! Functional readiness for every format our exports promise, without lavfi dependency.
use super::{Runtime, probes};
use anyhow::{Result, ensure};
use std::{
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

pub(super) const REQUIRED_ENCODERS: [&str; 2] = ["flac", "libmp3lame"];
const FRAMES: u32 = 4410;
const SAMPLE_RATE: u32 = 44100;

pub(super) fn verify(runtime: &Runtime, directory: &Path, cancel: &Arc<AtomicBool>) -> Result<()> {
    probes::version(&runtime.ffmpeg, "ffmpeg", cancel)?;
    probes::version(&runtime.ffprobe, "ffprobe", cancel)?;
    let input = tempfile::Builder::new()
        .suffix(".wav")
        .tempfile_in(directory)?;
    fs::write(input.path(), silence_wav())?;
    for (encoder, codec) in REQUIRED_ENCODERS.into_iter().zip(["flac", "mp3"]) {
        let mut encode = probes::command(&runtime.ffmpeg);
        encode
            .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-i"])
            .arg(input.path())
            .args(["-map", "0:a:0", "-c:a", encoder, "-f", codec, "pipe:1"]);
        let encoded = probes::collect(encode, 256 * 1024, cancel.clone())?;
        ensure!(
            !encoded.is_empty(),
            "FFmpeg {encoder} smoke encode was empty"
        );
        let sample = tempfile::Builder::new()
            .suffix(&format!(".{codec}"))
            .tempfile_in(directory)?;
        fs::write(sample.path(), encoded)?;
        let mut probe = probes::command(&runtime.ffprobe);
        probe
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_name,sample_rate,channels",
                "-of",
                "json",
            ])
            .arg(sample.path());
        let result: serde_json::Value =
            serde_json::from_slice(&probes::collect(probe, 64 * 1024, cancel.clone())?)?;
        let streams = result["streams"]
            .as_array()
            .filter(|s| s.len() == 1)
            .ok_or_else(|| anyhow::anyhow!("ffprobe {codec} smoke stream missing"))?;
        ensure!(
            streams[0]["codec_name"] == codec
                && streams[0]["sample_rate"]
                    .as_str()
                    .and_then(|rate| rate.parse::<u32>().ok())
                    == Some(SAMPLE_RATE)
                && streams[0]["channels"] == 1,
            "ffprobe {codec} smoke metadata mismatch"
        );
        let mut decode = probes::command(&runtime.ffmpeg);
        decode
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(sample.path())
            .args(["-f", "s16le", "-acodec", "pcm_s16le", "pipe:1"]);
        let decoded = probes::collect(decode, 16 * 1024, cancel.clone())?;
        let expected = (FRAMES * 2) as usize;
        ensure!(
            decoded.len() % 2 == 0 && decoded.len() >= expected && decoded.iter().all(|&b| b == 0),
            "FFmpeg {codec} smoke decode mismatch"
        );
        if codec == "flac" {
            ensure!(
                decoded.len() == expected,
                "FLAC smoke decode changed frame count"
            );
        }
    }
    Ok(())
}

fn silence_wav() -> Vec<u8> {
    let bytes = FRAMES * 2;
    let mut wav = Vec::with_capacity(bytes as usize + 44);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&bytes.to_le_bytes());
    wav.resize(bytes as usize + 44, 0);
    wav
}
