use super::{
    input, reference,
    request::{Limits, Operation},
};
use anyhow::{Result, ensure};
use mastercraft::{
    analysis::Analysis,
    audio::Audio,
    export,
    report::Report,
    studio::{doctor::IssueReport, spectrogram::Spectrogram},
    wav_encoding,
};
use serde::Serialize;
use serde_json::Value;
use std::{
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize)]
pub struct AudioFile {
    pub path: PathBuf,
    pub frames: usize,
    pub sample_rate: u32,
    pub channels: usize,
    pub duration_seconds: f64,
}
impl AudioFile {
    pub fn new(path: &Path, audio: &Audio) -> Self {
        Self {
            path: json_path(path),
            frames: audio.frames(),
            sample_rate: audio.sample_rate,
            channels: audio.channels,
            duration_seconds: audio.duration(),
        }
    }
}

fn json_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        // Python's pathlib does not equate verbatim and ordinary drive paths
        // during containment checks; canonicalization remains internal to Rust.
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        if let Some(ordinary) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(ordinary);
        }
    }
    path.to_path_buf()
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Original,
    Master,
    Preview,
    Stem,
}

#[derive(Serialize)]
pub struct Asset {
    pub role: Role,
    pub name: String,
    pub format: String,
    pub channel_peaks: Vec<f32>,
    #[serde(flatten)]
    pub audio: AudioFile,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<mastercraft::studio::stem_mix::StemKind>,
}
#[derive(Serialize)]
pub struct Waveform {
    pub peaks: Vec<[f32; 2]>,
    pub duration_seconds: f64,
    pub channel_peaks: Vec<f32>,
}

#[derive(Serialize)]
pub struct Suggestion {
    pub bands: Vec<mastercraft::config::EqBand>,
}

pub fn waveform(audio: &Audio, bins: usize) -> Waveform {
    let bins = bins.min(audio.frames());
    let mut channel_peaks = vec![0_f32; audio.channels];
    let peaks = (0..bins)
        .map(|index| {
            let start = (index * audio.frames() / bins) * audio.channels;
            let end = ((index + 1) * audio.frames() / bins) * audio.channels;
            audio.samples[start..end].iter().enumerate().fold(
                [f32::INFINITY, f32::NEG_INFINITY],
                |[low, high], (index, &v)| {
                    channel_peaks[index % audio.channels] =
                        channel_peaks[index % audio.channels].max(v.abs());
                    [low.min(v), high.max(v)]
                },
            )
        })
        .collect();
    Waveform {
        peaks,
        duration_seconds: audio.duration(),
        channel_peaks,
    }
}

#[derive(Serialize)]
pub struct Response {
    pub api_version: u32,
    pub operation: Operation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioFile>,
    pub assets: Vec<Asset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<Analysis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<Report>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issues: Option<IssueReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waveform: Option<Waveform>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spectrogram: Option<Spectrogram>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<Suggestion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_metadata: Option<Value>,
    pub resource_limits: Value,
}
impl Response {
    pub fn new(operation: Operation, limits: &Limits) -> Self {
        let mut resources = serde_json::to_value(limits).expect("validated finite limits");
        resources["separation_max_pcm_bytes"] = (512_u64 * 1024 * 1024).into();
        resources["zip_max_bytes"] = (1024_u64 * 1024 * 1024).into();
        resources["zip_entry_max_bytes"] = (512_u64 * 1024 * 1024).into();
        resources["memory_model"] =
            "Full input PCM plus multiple full-length DSP buffers; external RSS quota required"
                .into();
        Self {
            api_version: 2,
            operation,
            audio: None,
            assets: vec![],
            analysis: None,
            report: None,
            issues: None,
            waveform: None,
            spectrogram: None,
            suggestion: None,
            tool_metadata: None,
            resource_limits: resources,
        }
    }
    pub fn describe(&mut self, audio: &Audio, path: &Path, limits: &Limits) {
        self.audio = Some(AudioFile::new(path, audio));
        self.waveform = Some(waveform(audio, limits.waveform_bins));
    }
    pub fn validate(&self, limits: &Limits, out_dir: &Path) -> Result<()> {
        if let Some(analysis) = &self.analysis {
            reference::validate(analysis, limits)?;
        }
        if let Some(report) = &self.report {
            reference::validate(&report.input, limits)?;
            reference::validate(&report.output, limits)?;
        }
        if let Some(display) = &self.spectrogram {
            ensure!(
                display.levels_dbfs.iter().all(|x| x.is_finite()),
                "Non-finite spectrogram result"
            );
        }
        for asset in &self.assets {
            super::paths::no_links(&asset.audio.path)?;
            ensure!(
                asset.audio.path.canonicalize()?.starts_with(out_dir),
                "Asset escaped command output directory"
            );
        }
        Ok(())
    }
}

pub fn float(
    audio: &Audio,
    out_dir: &Path,
    name: &str,
    role: Role,
    limits: &Limits,
) -> Result<Asset> {
    input::validate(audio, limits, limits.max_pcm_bytes)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".pcm-")
        .suffix(".wav")
        .tempfile_in(out_dir)?;
    {
        let mut writer = BufWriter::new(temporary.as_file_mut());
        wav_encoding::write_wav(audio, &mut writer, "wav32f", false)?;
        writer.flush()?;
    }
    temporary.as_file().sync_all()?;
    let path = export::persist_unique(temporary, out_dir, name, "wav")?;
    Ok(Asset {
        role,
        name: name.into(),
        format: "wav32f".into(),
        channel_peaks: waveform(audio, 1).channel_peaks,
        audio: AudioFile::new(&path, audio),
        kind: None,
    })
}
