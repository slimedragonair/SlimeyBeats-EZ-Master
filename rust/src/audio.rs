use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    process::{Command, Stdio},
};

#[derive(Clone, Debug)]
pub struct Audio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: usize,
}

impl Audio {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: usize) -> Result<Self> {
        ensure!(
            (8000..=384000).contains(&sample_rate),
            "Sample rate must be 8000–384000 Hz"
        );
        ensure!(
            (1..=2).contains(&channels),
            "Only mono and stereo are supported; downmix explicitly first"
        );
        ensure!(
            !samples.is_empty() && samples.len().is_multiple_of(channels),
            "Empty or incomplete audio frames"
        );
        ensure!(
            samples.iter().all(|x| x.is_finite()),
            "Audio contains NaN or infinite samples"
        );
        Ok(Self {
            samples,
            sample_rate,
            channels,
        })
    }
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }
    pub fn duration(&self) -> f64 {
        self.frames() as f64 / self.sample_rate as f64
    }
    pub fn gain(&mut self, db: f64) {
        let gain = 10_f64.powf(db / 20.0) as f32;
        for sample in &mut self.samples {
            *sample *= gain;
        }
    }
    pub fn rms(&self) -> f64 {
        (self
            .samples
            .iter()
            .map(|&v| (v as f64).powi(2))
            .sum::<f64>()
            / self.samples.len() as f64)
            .sqrt()
    }
}

#[derive(Deserialize, Serialize)]
struct Probe {
    streams: Vec<Stream>,
}
#[derive(Deserialize, Serialize)]
struct Stream {
    sample_rate: String,
    channels: usize,
}

pub fn external_command(name: &str) -> Command {
    let cmd = Command::new(name);
    #[cfg(windows)]
    let mut cmd = cmd;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // Avoid console flashes from codecs launched by the GUI.
    }
    cmd
}

pub fn load(path: &Path) -> Result<Audio> {
    ensure!(path.is_file(), "Input does not exist: {}", path.display());
    if path
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("wav"))
    {
        return load_wav(path);
    }
    let probe = external_command("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=sample_rate,channels",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .context("Install FFmpeg and ffprobe on PATH to open compressed audio")?;
    ensure!(
        probe.status.success(),
        "ffprobe: {}",
        String::from_utf8_lossy(&probe.stderr)
    );
    let info: Probe = serde_json::from_slice(&probe.stdout).context("Invalid ffprobe response")?;
    let stream = info.streams.first().context("No audio stream")?;
    ensure!(
        (1..=2).contains(&stream.channels),
        "Input has {} channels. Explicitly downmix to mono or stereo first",
        stream.channels
    );
    let decoded = external_command("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args([
            "-map",
            "0:a:0",
            "-f",
            "f32le",
            "-acodec",
            "pcm_f32le",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .output()
        .context("Cannot start FFmpeg")?;
    ensure!(
        decoded.status.success(),
        "Decode failed: {}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    ensure!(
        decoded.stdout.len().is_multiple_of(4),
        "Truncated decoded audio"
    );
    let samples = decoded
        .stdout
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    Audio::new(samples, stream.sample_rate.parse()?, stream.channels)
}

fn load_wav(path: &Path) -> Result<Audio> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("Read {}", path.display()))?;
    let spec = reader.spec();
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 2_f64.powi(spec.bits_per_sample as i32 - 1);
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| (v as f64 / scale) as f32))
                .collect::<std::result::Result<Vec<_>, _>>()?
        }
    };
    let expected = reader.duration() as usize * spec.channels as usize;
    if samples.len() != expected {
        bail!(
            "Truncated WAV: expected {expected} samples, got {}",
            samples.len()
        );
    }
    Audio::new(samples, spec.sample_rate, spec.channels as usize)
}

pub const EXTENSIONS: &[&str] = &[
    "wav", "flac", "mp3", "ogg", "oga", "aiff", "aif", "m4a", "aac", "opus", "wma",
];
pub fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
}
