use anyhow::{Context, Result, ensure};
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

const CONFIG_LIMIT: usize = 256 * 1024;
const CONFIG_TIMEOUT: Duration = Duration::from_secs(5);
enum ConfigEvent {
    Progress(String),
    Done(Result<String, String>),
}

#[derive(Default)]
pub(super) struct McpSetup {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub config: Option<String>,
    pub error: Option<String>,
    pub copied: bool,
    pub status: String,
    receiver: Option<mpsc::Receiver<ConfigEvent>>,
    cancel: Option<Arc<AtomicBool>>,
}

pub(super) fn companion(root: &Path) -> Option<PathBuf> {
    let name = format!("slimeybeats-mcp{}", std::env::consts::EXE_SUFFIX);
    super::bundled_resources::file(
        root,
        &[
            PathBuf::from(&name),
            PathBuf::from("mcp/server/target/release").join(&name),
            PathBuf::from("mcp/server/target/debug").join(name),
        ],
    )
    .or_else(|| crate::component_tools::locate(crate::component_tools::ToolKind::McpBridge))
}

fn config_command(binary: &Path, desktop: &Path, input: &Path, output: &Path) -> Result<Command> {
    ensure!(input.is_dir(), "Choose an existing audio folder first");
    ensure!(
        !output.as_os_str().is_empty(),
        "Choose an output folder first"
    );
    ensure!(
        binary.is_file(),
        "The MCP companion was not found in this build"
    );
    let mut command = crate::audio::external_command(
        binary
            .to_str()
            .context("The companion path is not valid Unicode")?,
    );
    command
        .arg("--input-root")
        .arg(input.canonicalize()?)
        .arg("--output-root")
        .arg(std::path::absolute(output)?)
        .arg("--desktop-exe")
        .arg(desktop)
        .arg("--profiles-root")
        .arg(crate::paths::app_root().join("profiles"))
        .arg("--print-config");
    Ok(command)
}

impl McpSetup {
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }

    pub fn invalidate(&mut self) {
        self.cancel();
        self.config = None;
        self.error = None;
        self.copied = false;
        self.status.clear();
    }

    pub fn generate(&mut self, binary: &Path, ctx: &egui::Context) {
        self.generate_or_prepare(Some(binary.to_path_buf()), ctx);
    }
    pub fn generate_or_prepare(&mut self, binary: Option<PathBuf>, ctx: &egui::Context) {
        self.invalidate();
        let request = (|| {
            let desktop = std::env::current_exe()?;
            let input = self.input.clone().context("Choose an audio folder")?;
            let output = self.output.clone().context("Choose an output folder")?;
            ensure!(input.is_dir(), "Choose an existing audio folder first");
            ensure!(
                !output.as_os_str().is_empty(),
                "Choose an output folder first"
            );
            anyhow::Ok((desktop, input, output))
        })();
        let (desktop, input, output) = match request {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        self.receiver = Some(receiver);
        self.status = "Preparing your connection example…".into();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<String> {
                let binary = match binary {
                    Some(binary) => binary,
                    None => crate::component_tools::ensure(
                        crate::component_tools::ToolKind::McpBridge,
                        cancel.clone(),
                        &|_, message| {
                            let _ = sender.send(ConfigEvent::Progress(message.into()));
                            ctx.request_repaint();
                        },
                    )?,
                };
                let command = config_command(&binary, &desktop, &input, &output)?;
                let mut bytes = vec![];
                crate::native_process::read_stdout(
                    command,
                    CONFIG_LIMIT,
                    CONFIG_TIMEOUT,
                    cancel,
                    |chunk| {
                        bytes.extend_from_slice(chunk);
                        Ok(())
                    },
                )?;
                let value: serde_json::Value = serde_json::from_slice(&bytes)
                    .context("The companion did not return a valid configuration")?;
                ensure!(
                    value["mcpServers"]["slimeybeats"]["command"].is_string()
                        && value["mcpServers"]["slimeybeats"]["args"].is_array(),
                    "The companion returned an unexpected configuration format"
                );
                Ok(serde_json::to_string_pretty(&value)?)
            })()
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send(ConfigEvent::Done(result));
            ctx.request_repaint();
        });
    }

    pub fn poll(&mut self) {
        loop {
            let Some(receiver) = &self.receiver else {
                return;
            };
            match receiver.try_recv() {
                Ok(ConfigEvent::Progress(message)) => self.status = message,
                Ok(ConfigEvent::Done(result)) => {
                    self.receiver = None;
                    self.cancel = None;
                    match result {
                        Ok(config) => {
                            self.config = Some(config);
                            self.status = "Connection example ready.".into();
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.status.clear();
                        }
                    }
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.cancel = None;
                    self.error = Some(
                    "The setup helper stopped before returning a connection example. Try again."
                        .into(),
                );
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => return,
            }
        }
    }

    pub fn cancel(&mut self) {
        self.status = "Companion setup cancelled. Your audio is unchanged.".into();
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.receiver = None;
    }
}

impl Drop for McpSetup {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_generation_is_explicit_bounded_and_keeps_literal_unicode_paths() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("音源 é ; literal");
        let output = folder.path().join("Exports 龍");
        std::fs::create_dir(&input).unwrap();
        let executable = std::env::current_exe().unwrap();
        let command = config_command(&executable, &executable, &input, &output).unwrap();
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args[0], "--input-root");
        assert_eq!(args[1], input.canonicalize().unwrap());
        assert_eq!(args[3], output);
        assert_eq!(
            args.last().unwrap(),
            &std::ffi::OsStr::new("--print-config")
        );
        assert!(
            !output.exists(),
            "Preparing connection arguments must not create folders or start a server"
        );
        assert!(config_command(&executable, &executable, &input.join("missing"), &output).is_err());
        let bundled = folder
            .path()
            .join(format!("slimeybeats-mcp{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&bundled, b"fixture").unwrap();
        assert_eq!(companion(folder.path()), Some(bundled));
    }
}
