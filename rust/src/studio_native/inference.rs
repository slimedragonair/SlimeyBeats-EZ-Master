use super::runtime::StemRuntime;
use crate::{
    audio::Audio,
    native_process,
    studio::stem_mix::{self, Stem, StemKind, StemSet},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const MAX_INPUT_BYTES: usize = 512 * 1024 * 1024;
const MAX_PROCESS_SECONDS: u64 = 3600;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub enum SeparationMode {
    #[default]
    VocalInstrumental,
    FourStems,
}

pub struct Separated {
    pub stems: StemSet,
    pub metadata: Value,
}

impl StemRuntime {
    pub fn separate(
        &self,
        input: &Audio,
        mode: SeparationMode,
        work_root: &Path,
        progress: &dyn Fn(f32, &str),
        cancel: Arc<AtomicBool>,
    ) -> Result<Separated> {
        let _lock = if let Some(path) = &self.lock_path {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(path)?;
            file.try_lock()
                .context("Another local stem job/setup is active; wait for it and retry")?;
            Some(file)
        } else {
            None
        };
        ensure!(
            self.python.is_file() && self.models.join("model.index").is_file(),
            "Install the local stem engine first"
        );
        // Keep a virtualenv executable's symlink intact; resolving it on Unix
        // would bypass pyvenv.cfg and its isolated installed packages.
        let python = std::path::absolute(&self.python)?;
        let models = self.models.canonicalize()?;
        ensure!(
            input.samples.len() <= MAX_INPUT_BYTES / 4,
            "Separation input exceeds 512 MiB decoded PCM; use a shorter or lower-rate source"
        );
        ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
        std::fs::create_dir_all(work_root)?;
        let directory = tempfile::Builder::new()
            .prefix(".studio-separation-")
            .tempdir_in(work_root)?;
        let input_path = directory.path().join("input.f32");
        let output = directory.path().join("stems");
        let worker = directory.path().join("inference.py");
        std::fs::write(&worker, include_str!("inference_worker.py"))?;
        let mut file = BufWriter::new(File::create(&input_path)?);
        for chunk in input.samples.chunks(16384) {
            ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
            for &sample in chunk {
                file.write_all(&sample.to_le_bytes())?;
            }
        }
        file.flush()?;
        drop(file);
        let request = directory.path().join("request.json");
        std::fs::write(
            &request,
            serde_json::to_vec(
                &json!({"input_pcm":input_path,"output_directory":output,"model_directory":models,"sample_rate":input.sample_rate,"channels":input.channels,"frames":input.frames()}),
            )?,
        )?;
        let mut command = crate::audio::external_command(
            python
                .to_str()
                .context("Runtime executable path must be Unicode")?,
        );
        command
            .arg("-I")
            .arg(&worker)
            .arg(&request)
            .current_dir(directory.path());
        command
            .env_remove("GITHUB_HOST")
            .env_remove("GITHUB_REPOSITORY")
            .env_remove("GITHUB_RELEASE");
        let mut pending = Vec::new();
        let mut worker_error = None;
        let result = native_process::read_stdout(
            command,
            1024 * 1024,
            Duration::from_secs(MAX_PROCESS_SECONDS),
            cancel.clone(),
            |chunk| {
                pending.extend_from_slice(chunk);
                while let Some(end) = pending.iter().position(|&b| b == b'\n') {
                    ensure!(end <= 16384, "Stem worker returned an oversized event");
                    let line = pending.drain(..=end).collect::<Vec<_>>();
                    let event: Value = serde_json::from_slice(&line)
                        .context("Unexpected local stem worker output")?;
                    match event["event"].as_str() {
                        Some("progress") => progress(
                            event["fraction"].as_f64().unwrap_or(0.).clamp(0., 0.95) as f32,
                            event["message"].as_str().unwrap_or("Separating locally"),
                        ),
                        Some("error") => {
                            worker_error = Some(
                                event["message"]
                                    .as_str()
                                    .unwrap_or("Local model failed")
                                    .to_string(),
                            )
                        }
                        _ => {}
                    }
                }
                ensure!(
                    pending.len() <= 16384,
                    "Stem worker returned an oversized event"
                );
                Ok(())
            },
        );
        if let Some(error) = worker_error {
            anyhow::bail!("{error}");
        }
        result?;
        ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
        progress(0.96, "Checking and aligning the separated stems");
        let report_path = output.join("result.json");
        ensure!(
            report_path.metadata()?.len() < 65536,
            "Stem result metadata is too large"
        );
        let metadata: Value = serde_json::from_slice(&std::fs::read(report_path)?)?;
        ensure!(
            metadata["frames"] == input.frames()
                && metadata["sample_rate"] == input.sample_rate
                && metadata["channels"] == input.channels,
            "Model output layout does not match the input"
        );
        let mut estimates = Vec::new();
        let kinds = match mode {
            SeparationMode::VocalInstrumental => vec![StemKind::Vocals],
            SeparationMode::FourStems => vec![StemKind::Vocals, StemKind::Drums, StemKind::Bass],
        };
        for kind in kinds {
            let samples = read_pcm(
                &output.join(format!("{}.f32", kind.name())),
                input.samples.len(),
                &cancel,
            )?;
            estimates.push(Stem {
                kind,
                audio: Arc::new(Audio::new(samples, input.sample_rate, input.channels)?),
            });
        }
        let stems = stem_mix::consistent(
            input,
            estimates,
            matches!(mode, SeparationMode::VocalInstrumental),
        )?;
        ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
        Ok(Separated { stems, metadata })
    }
}

fn read_pcm(path: &Path, samples: usize, cancel: &AtomicBool) -> Result<Vec<f32>> {
    ensure!(
        path.metadata()?.len() == samples as u64 * 4,
        "A stem has an incomplete or oversized PCM layout"
    );
    let mut result = Vec::new();
    result
        .try_reserve_exact(samples)
        .context("Not enough memory for the separated stems")?;
    let mut file = BufReader::new(File::open(path)?);
    let mut chunk = [0_u8; 65536];
    loop {
        ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        ensure!(count.is_multiple_of(4), "Truncated stem PCM sample");
        for bytes in chunk[..count].as_chunks::<4>().0 {
            result.push(f32::from_le_bytes(*bytes));
        }
    }
    ensure!(result.len() == samples, "Truncated stem PCM");
    Ok(result)
}
