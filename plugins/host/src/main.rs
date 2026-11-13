mod host_context;
mod instance;
mod module;
mod output_channel;
mod render;
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "Isolated local VST3 effect worker")]
struct Args {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Probe {
        plugin: PathBuf,
    },
    Render {
        plugin: PathBuf,
        #[arg(long)]
        class_id: Option<String>,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "[]")]
        parameters: String,
    },
}
fn main() -> Result<()> {
    let args = Args::parse();
    let mut response =
        output_channel::reserve().context("Cannot reserve the plugin response channel")?;
    let result = match args.command {
        Action::Probe { plugin } => {
            let library = module::Module::open(&plugin)?;
            let classes = library.classes()?;
            let mut results = Vec::new();
            for class in classes {
                let info = match instance::Instance::new(&library, &module::parse_id(&class.id)?) {
                    Ok(plugin) => {
                        json!({"class":class,"parameters":plugin.parameters()?,"supported_sample_size":"float32","editor":"generic parameters; no embedded vendor editor"})
                    }
                    Err(error) => json!({"class":class,"error":format!("{error:#}")}),
                };
                results.push(info);
            }
            json!({"plugins":results,"note":"Probing executes trusted plugin code in an isolated process, not an OS security sandbox."})
        }
        Action::Render {
            plugin,
            class_id,
            input,
            output,
            parameters,
        } => {
            ensure!(
                parameters.len() <= 1024 * 1024,
                "Parameter request is too large"
            );
            let parameters: Vec<render::ParameterValue> = serde_json::from_str(&parameters)?;
            let library = module::Module::open(&plugin)?;
            let id = class_id
                .or_else(|| {
                    library
                        .classes()
                        .ok()
                        .and_then(|c| c.first().map(|c| c.id.clone()))
                })
                .context("No audio effect class")?;
            let plugin = instance::Instance::new(&library, &module::parse_id(&id)?)?;
            let input = mastercraft::audio::load(&input)?;
            let (rendered, latency) = render::process(&plugin, &input, &parameters)?;
            let file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .read(true)
                .open(&output)?;
            mastercraft::wav_encoding::write_wav(&rendered, file, "wav32f", false)?;
            json!({"class_id":id,"frames":rendered.frames(),"channels":rendered.channels,"sample_rate":rendered.sample_rate,"latency_frames_compensated":latency,"tail_policy":"trimmed to original duration","parameters":parameters})
        }
    };
    writeln!(response, "{}", serde_json::to_string(&result)?)?;
    response.flush()?;
    Ok(())
}
