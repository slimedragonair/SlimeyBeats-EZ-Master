use super::{
    download,
    inference::SeparationMode,
    runtime::{MODEL_SHA256, MODEL_URL, RUNTIME_API, RuntimePaths, StemRuntime},
};
use crate::{audio::Audio, native_process};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    path::Path,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

impl RuntimePaths {
    pub fn install(&self, progress: &dyn Fn(f32, &str), cancel: Arc<AtomicBool>) -> Result<Value> {
        ensure!(
            cfg!(all(
                target_arch = "x86_64",
                any(windows, target_os = "linux")
            )),
            "Automatic stem setup currently supports Windows/Linux x64"
        );
        std::fs::create_dir_all(&self.base)?;
        let marker = self.base.join("studio-owner.json");
        if !marker.exists() {
            ensure!(
                std::fs::read_dir(&self.base)?.next().is_none() || self.is_ready(),
                "Choose a dedicated empty stem-engine folder; existing unrelated files were preserved"
            );
            std::fs::write(
                &marker,
                serde_json::to_vec(
                    &json!({"owner":"SlimeyBeats-EZ-Master","runtime_api":RUNTIME_API}),
                )?,
            )?;
        } else {
            ensure!(
                marker.metadata()?.len() < 1024,
                "Invalid stem-engine ownership marker"
            );
            let owner: Value = serde_json::from_slice(&std::fs::read(&marker)?)?;
            ensure!(
                owner["owner"] == "SlimeyBeats-EZ-Master",
                "This folder belongs to another application"
            );
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.base.join("engine.lock"))?;
        lock.try_lock()
            .context("Another local stem setup is active; wait for it and retry")?;
        if self.is_ready() {
            return Ok(self.status());
        }
        let (asset, sha, relative) = if cfg!(windows) {
            (
                "uv-x86_64-pc-windows-msvc.zip",
                "93a0a244f26eec208d8ea8077e3de743d9687ad76c190342722f1f57fa629457",
                "uv.exe",
            )
        } else {
            (
                "uv-x86_64-unknown-linux-gnu.tar.gz",
                "e170aed70ac0225feee612e855d3a57ae73c61ffb22c7e52c3fd33b87c286508",
                "uv-x86_64-unknown-linux-gnu/uv",
            )
        };
        progress(0.02, "Preparing the private local stem engine");
        let archive = download::fetch(
            &format!("https://github.com/astral-sh/uv/releases/download/0.9.22/{asset}"),
            sha,
            &self.base.join(asset),
            64 * 1024 * 1024,
            cancel.clone(),
            &|bytes| {
                progress(
                    0.03,
                    &format!("Downloading setup helper · {} MiB", bytes / (1024 * 1024)),
                )
            },
        )?;
        let bootstrap = tempfile::Builder::new()
            .prefix(".engine-setup-")
            .tempdir_in(&self.base)?;
        let mut tar = crate::audio::external_command(if cfg!(windows) { "tar.exe" } else { "tar" });
        tar.arg("-xf").arg(&archive).arg("-C").arg(bootstrap.path());
        run(tar, &cancel)?;
        let uv = bootstrap.path().join(relative);
        ensure!(
            uv.is_file(),
            "The setup archive contains no compatible helper"
        );
        let mut create = uv_command(&uv, &self.base);
        create
            .args(["venv", "--managed-python", "--python", "3.11.14"])
            .arg(self.base.join("env"));
        if !self.python().is_file() {
            progress(
                0.08,
                "Installing isolated Python for the optional stem engine",
            );
            run(create, &cancel)?;
        }
        let requirements = bootstrap.path().join("requirements.txt");
        let overrides = bootstrap.path().join("overrides.txt");
        std::fs::write(&requirements, include_str!("runtime_requirements.txt"))?;
        std::fs::write(&overrides, include_str!("runtime_overrides.txt"))?;
        let mut dependencies = uv_command(&uv, &self.base);
        dependencies
            .args(["pip", "install", "--python"])
            .arg(self.python())
            .arg("--requirements")
            .arg(requirements)
            .arg("--override")
            .arg(overrides);
        progress(
            0.25,
            "Installing the local separation engine; this first setup can take several minutes",
        );
        run(dependencies, &cancel)?;
        progress(0.65, "Downloading the verified four-stem model");
        let model = download::fetch(
            MODEL_URL,
            MODEL_SHA256,
            &self.base.join("4stems.tar.gz"),
            256 * 1024 * 1024,
            cancel.clone(),
            &|bytes| {
                progress(
                    (0.65 + 0.2 * bytes as f32 / 146308147.).min(0.85),
                    &format!(
                        "Downloading stem model · {} / 140 MiB",
                        bytes / (1024 * 1024)
                    ),
                )
            },
        )?;
        let model_script = bootstrap.path().join("install_model.py");
        std::fs::write(&model_script, include_str!("model_install.py"))?;
        let mut extract = crate::audio::external_command(
            self.python()
                .to_str()
                .context("Engine path must be Unicode")?,
        );
        extract
            .arg("-I")
            .arg(model_script)
            .arg(model)
            .arg(self.models());
        progress(0.86, "Verifying and unpacking the model checkpoint");
        run(extract, &cancel)?;
        progress(0.9, "Testing the installed stem engine");
        let runtime = StemRuntime::configured(self.python(), self.models());
        let silence = Audio::new(vec![0.; 44100 * 2], 44100, 2)?;
        runtime
            .separate(
                &silence,
                SeparationMode::VocalInstrumental,
                bootstrap.path(),
                &|_, _| {},
                cancel.clone(),
            )
            .context("The installed engine did not pass its local inference test")?;
        ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
        let receipt = json!({"runtime_api":RUNTIME_API,"model_sha256":MODEL_SHA256,"engine":"spleeter","engine_version":"2.4.2","tensorflow":"2.12.1","python":"3.11.14","uv":"0.9.22","windows_gcs_override":"0.31.0 (unused by local inference)"});
        let mut file = tempfile::NamedTempFile::new_in(&self.base)?;
        serde_json::to_writer_pretty(&mut file, &receipt)?;
        file.as_file().sync_all()?;
        file.persist(self.receipt())?;
        Ok(self.status())
    }
}

fn uv_command(uv: &Path, base: &Path) -> Command {
    let mut command =
        crate::audio::external_command(uv.to_str().expect("owned engine helper path"));
    command
        .env("UV_CACHE_DIR", base.join("download-cache"))
        .env("UV_PYTHON_INSTALL_DIR", base.join("python"))
        .env("UV_NO_PROGRESS", "1")
        .env("UV_LOG", "error");
    command
}
fn run(command: Command, cancel: &Arc<AtomicBool>) -> Result<()> {
    native_process::read_stdout(
        command,
        1024 * 1024,
        Duration::from_secs(1800),
        cancel.clone(),
        |_| Ok(()),
    )
}
