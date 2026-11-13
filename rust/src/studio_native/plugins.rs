//! Local VST3 modules are probed/rendered only inside the optional host helper.
use crate::{audio::Audio, native_process};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PluginValue {
    pub id: u32,
    pub normalized: f64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PluginClass {
    pub id: String,
    pub name: String,
    pub category: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PluginParameter {
    pub id: u32,
    pub name: String,
    pub units: String,
    pub normalized: f64,
    pub default_normalized: f64,
    pub read_only: bool,
    pub steps: i32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub class: PluginClass,
    #[serde(default)]
    pub parameters: Vec<PluginParameter>,
    pub error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Probe {
    pub plugins: Vec<PluginInfo>,
}
#[derive(Clone)]
pub struct VstHost {
    pub executable: PathBuf,
}

impl VstHost {
    pub fn find() -> Option<Self> {
        let executable = std::env::current_exe().ok()?;
        let name = if cfg!(windows) {
            "slimeybeats-vst3-host.exe"
        } else {
            "slimeybeats-vst3-host"
        };
        for parent in executable.ancestors().skip(1).take(6) {
            for candidate in [
                parent.join(name),
                parent.join("plugins/host/target/release").join(name),
            ] {
                if candidate.is_file() {
                    return Some(Self {
                        executable: candidate,
                    });
                }
            }
        }
        None
    }
    pub fn probe(&self, module: &Path, cancel: Arc<AtomicBool>) -> Result<Probe> {
        let module = resolve_binary(module)?;
        let mut command = self.command()?;
        command.arg("probe").arg(module);
        let output = run(command, Duration::from_secs(30), cancel)?;
        serde_json::from_slice(&output).context("Plugin helper returned invalid probe metadata")
    }
    pub fn render(
        &self,
        module: &Path,
        class_id: &str,
        input: &Audio,
        values: &[PluginValue],
        work: &Path,
        cancel: Arc<AtomicBool>,
    ) -> Result<(Audio, Value)> {
        ensure!(values.len() <= 4096, "Too many plugin parameter changes");
        ensure!(
            values
                .iter()
                .all(|v| v.normalized.is_finite() && (0. ..=1.).contains(&v.normalized)),
            "Plugin values must be normalized 0–1"
        );
        let module = resolve_binary(module)?;
        std::fs::create_dir_all(work)?;
        let directory = tempfile::Builder::new()
            .prefix(".studio-plugin-")
            .tempdir_in(work)?;
        let source = directory.path().join("input.wav");
        let output = directory.path().join("output.wav");
        crate::wav_encoding::write_wav(input, std::fs::File::create(&source)?, "wav32f", false)?;
        let mut command = self.command()?;
        command
            .arg("render")
            .arg(module)
            .arg("--class-id")
            .arg(class_id)
            .arg("--input")
            .arg(&source)
            .arg("--output")
            .arg(&output)
            .arg("--parameters")
            .arg(serde_json::to_string(values)?);
        let bytes = run(command, Duration::from_secs(600), cancel.clone())?;
        let metadata: Value =
            serde_json::from_slice(&bytes).context("Invalid plugin render metadata")?;
        let (audio, _) = super::audio_import::load(
            &output,
            input.samples.len().saturating_mul(4).saturating_add(1024),
            Duration::from_secs(30),
            cancel,
        )?;
        ensure!(
            audio.frames() == input.frames()
                && audio.channels == input.channels
                && audio.sample_rate == input.sample_rate,
            "Plugin helper changed the declared audio layout"
        );
        Ok((audio, metadata))
    }
    fn command(&self) -> Result<Command> {
        Ok(crate::audio::external_command(
            self.executable
                .to_str()
                .context("Plugin host path must be Unicode")?,
        ))
    }
}
fn run(command: Command, timeout: Duration, cancel: Arc<AtomicBool>) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    native_process::read_stdout(command, 1024 * 1024, timeout, cancel, |chunk| {
        output.extend_from_slice(chunk);
        Ok(())
    })?;
    Ok(output)
}

pub fn resolve_binary(module: &Path) -> Result<PathBuf> {
    let module = module
        .canonicalize()
        .context("Plugin path is unavailable")?;
    if module.is_file() {
        return Ok(module);
    }
    let architecture = if cfg!(windows) {
        "x86_64-win"
    } else if cfg!(target_os = "linux") {
        "x86_64-linux"
    } else {
        anyhow::bail!("This plugin helper build supports Windows/Linux x64")
    };
    let directory = module.join("Contents").join(architecture);
    let files = std::fs::read_dir(directory)
        .context("Select a compatible x64 VST3 bundle")?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && p.extension().is_some_and(|e| {
                    e.eq_ignore_ascii_case("vst3")
                        || e.eq_ignore_ascii_case("so")
                        || e.eq_ignore_ascii_case("dll")
                })
        })
        .collect::<Vec<_>>();
    ensure!(
        files.len() == 1,
        "Bundle must contain exactly one compatible plugin binary"
    );
    Ok(files[0].canonicalize()?)
}
