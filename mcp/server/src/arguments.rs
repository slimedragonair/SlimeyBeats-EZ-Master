use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Local SlimeyBeats MCP companion. Audio processing stays on this computer."
)]
pub struct Arguments {
    /// Folders the assistant may read. Repeat for additional libraries.
    #[arg(long, required = true, num_args = 1)]
    pub input_root: Vec<PathBuf>,
    /// Dedicated folder for new exports, session files and temporary previews.
    #[arg(long)]
    pub output_root: PathBuf,
    /// Maximum decoded input PCM size; processing needs additional memory.
    #[arg(long, default_value_t = 512, value_parser = clap::value_parser!(u32).range(8..=4096))]
    pub max_decoded_mib: u32,
    /// Timeout for an individual external decoder/probe process.
    #[arg(long, default_value_t = 300, value_parser = clap::value_parser!(u64).range(5..=3600))]
    pub decode_timeout_seconds: u64,
    /// Trusted desktop executable used only by the explicit open_in_desktop tool.
    #[arg(long)]
    pub desktop_exe: Option<PathBuf>,
    /// Print a client configuration example and exit; do not start the server.
    #[arg(long)]
    pub print_config: bool,
}

impl Arguments {
    pub fn client_config(&self) -> anyhow::Result<serde_json::Value> {
        let mut args = Vec::<String>::new();
        for root in &self.input_root {
            args.extend([
                "--input-root".into(),
                root.canonicalize()?.to_string_lossy().into_owned(),
            ]);
        }
        args.extend([
            "--output-root".into(),
            std::path::absolute(&self.output_root)?
                .to_string_lossy()
                .into_owned(),
            "--max-decoded-mib".into(),
            self.max_decoded_mib.to_string(),
            "--decode-timeout-seconds".into(),
            self.decode_timeout_seconds.to_string(),
        ]);
        if let Some(path) = &self.desktop_exe {
            args.extend([
                "--desktop-exe".into(),
                path.canonicalize()?.to_string_lossy().into_owned(),
            ]);
        }
        Ok(
            serde_json::json!({"mcpServers":{"slimeybeats":{"command":std::env::current_exe()?,"args":args}}}),
        )
    }
}
