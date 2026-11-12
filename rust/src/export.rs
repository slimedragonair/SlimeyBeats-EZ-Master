use crate::{
    audio::{self, Audio, external_command},
    config::Settings,
    dsp::limiter,
    meters,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::{Builder, NamedTempFile};

fn wav(audio: &Audio, path: &Path, format: &str, dither: bool) -> Result<()> {
    let float = format == "wav32f";
    let bits = if float {
        32
    } else if format.ends_with("16") {
        16
    } else {
        24
    };
    let spec = hound::WavSpec {
        channels: audio.channels as u16,
        sample_rate: audio.sample_rate,
        bits_per_sample: bits,
        sample_format: if float {
            hound::SampleFormat::Float
        } else {
            hound::SampleFormat::Int
        },
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    let scale = 2_f64.powi(bits as i32 - 1);
    let mut random = 0x6d61737465726372_u64;
    let mut uniform = || {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        (random >> 11) as f64 / (1_u64 << 53) as f64
    };
    for &sample in &audio.samples {
        if float {
            writer.write_sample(sample)?;
        } else {
            let noise = if dither { uniform() - uniform() } else { 0. };
            let quantized = (sample as f64 * scale + noise)
                .round()
                .clamp(-scale, scale - 1.) as i32;
            writer.write_sample(quantized)?;
        }
    }
    writer.finalize()?;
    Ok(())
}
fn temporary(dir: &Path, suffix: &str) -> Result<NamedTempFile> {
    Ok(Builder::new()
        .prefix(".mastercraft-")
        .suffix(suffix)
        .tempfile_in(dir)?)
}

pub fn resample(input: &Audio, rate: u32, dir: &Path) -> Result<Audio> {
    if rate == 0 || rate == input.sample_rate {
        return Ok(input.clone());
    }
    let source = temporary(dir, ".wav")?;
    wav(input, source.path(), "wav32f", false)?;
    let dest = temporary(dir, ".wav")?;
    let out = external_command("ffmpeg")
        .args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(source.path())
        .args(["-ar", &rate.to_string(), "-c:a", "pcm_f32le", "-f", "wav"])
        .arg(dest.path())
        .output()
        .context("FFmpeg is required to change sample rate")?;
    ensure!(
        out.status.success(),
        "Resampling failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    audio::load(dest.path())
}

fn encode(audio: &Audio, path: &Path, format: &str, dither: bool, dir: &Path) -> Result<()> {
    if format.starts_with("wav") {
        return wav(audio, path, format, dither);
    }
    let source = temporary(dir, ".wav")?;
    let intermediary = if format == "mp3" {
        "wav32f"
    } else if format.ends_with("16") {
        "wav16"
    } else {
        "wav24"
    };
    wav(audio, source.path(), intermediary, dither)?;
    let mut cmd = external_command("ffmpeg");
    cmd.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(source.path());
    if format == "mp3" {
        cmd.args(["-c:a", "libmp3lame", "-b:a", "320k", "-f", "mp3"]);
    } else {
        cmd.args(["-c:a", "flac", "-compression_level", "5", "-f", "flac"]);
    }
    let result = cmd
        .arg(path)
        .output()
        .context("FFmpeg is required for FLAC and MP3 exports")?;
    ensure!(
        result.status.success(),
        "Encoding failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

fn output_parts(format: &str) -> (&str, &str) {
    match format {
        "wav16" => ("16bit", "wav"),
        "wav32f" => ("32f", "wav"),
        "flac16" => ("16bit", "flac"),
        "flac24" => ("", "flac"),
        "mp3" => ("", "mp3"),
        _ => ("", "wav"),
    }
}

/// Persist without replacing an existing file, including simultaneous jobs with the same basename.
pub fn persist_unique(
    mut temp: NamedTempFile,
    dir: &Path,
    stem: &str,
    extension: &str,
) -> Result<PathBuf> {
    for index in 0..10000 {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!("_{}", index + 1)
        };
        let path = dir.join(format!("{stem}{suffix}.{extension}"));
        match temp.persist_noclobber(&path) {
            Ok(_) => return Ok(path),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                temp = error.file
            }
            Err(error) => return Err(error.error.into()),
        }
    }
    bail!("Could not allocate a free output name after 10000 collisions")
}

pub fn export(
    audio: &Audio,
    input_path: &Path,
    s: &Settings,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>> {
    std::fs::create_dir_all(&s.out_dir)
        .with_context(|| format!("Create {}", s.out_dir.display()))?;
    let mut working = resample(audio, s.output_sr, &s.out_dir)?;
    limiter::guard(&mut working, s.ceiling_dbtp)?;
    let name = input_path.file_stem().unwrap_or_default().to_string_lossy();
    let mut results = Vec::new();
    for format in &s.formats {
        ensure!(!cancel(), "Cancelled");
        let (tag, ext) = output_parts(format);
        let base = format!(
            "{name}{}{}",
            s.suffix,
            if tag.is_empty() {
                String::new()
            } else {
                format!("_{tag}")
            }
        );
        let temp = temporary(&s.out_dir, &format!(".{ext}"))?;
        let mut source = working.clone();
        let mut passes = 0;
        let metrics = loop {
            passes += 1;
            encode(&source, temp.path(), format, s.dither, &s.out_dir)?;
            let decoded = audio::load(temp.path())?;
            let measured = meters::measure(&decoded)?;
            if measured.true_peak_dbtp <= s.ceiling_dbtp + 0.005 {
                break measured;
            }
            ensure!(
                passes < 5,
                "Decoded export still exceeds the ceiling after {passes} encodes"
            );
            source.gain(s.ceiling_dbtp - 0.15 - measured.true_peak_dbtp);
            ensure!(!cancel(), "Cancelled");
        };
        temp.as_file().sync_all()?;
        let path = persist_unique(temp, &s.out_dir, &base, ext)?;
        results.push(json!({"path":path,"format":format,"sample_rate":source.sample_rate,"encode_passes":passes,
            "decoded_metrics":metrics,"dither":s.dither&&format!="wav32f"&&format!="mp3"}));
    }
    Ok(results)
}
pub fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut temp = temporary(dir, ".json")?;
    serde_json::to_writer_pretty(&mut temp, value)?;
    temp.write_all(b"\n")?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path)
        .map_err(|e| e.error)
        .with_context(|| {
            format!(
                "Cannot create {} (existing files are preserved)",
                path.display()
            )
        })?;
    Ok(())
}
