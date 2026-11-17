mod arrangement;
mod input;
mod module_settings;
mod output;
mod paths;
mod processing;
mod reference;
mod request;
mod settings;
mod stems;

use anyhow::{Context, Result, ensure};
use clap::Parser;
use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(about = "Private server DSP worker; see hybrid_worker/README.md")]
struct Arguments {
    #[arg(long, required_unless_present = "arrangement_server")]
    request: Option<PathBuf>,
    #[arg(long, required_unless_present = "arrangement_server")]
    response: Option<PathBuf>,
    #[arg(long, conflicts_with_all = ["request", "response"])]
    arrangement_server: Option<PathBuf>,
    #[arg(long, default_value_t = 4, value_parser = clap::value_parser!(u16).range(1..=256))]
    threads: u16,
}

pub fn progress(fraction: f32, message: &'static str) {
    let line = serde_json::json!({"progress":fraction.clamp(0.,1.),"message":message});
    // All callers supply fixed literals; upstream messages/paths never reach stdout.
    let mut stdout = std::io::stdout().lock();
    if let Err(error) = writeln!(stdout, "{line}").and_then(|_| stdout.flush()) {
        eprintln!("Cannot report worker progress: {error}");
    }
}

pub fn run() -> Result<()> {
    let args = Arguments::parse();
    if let Some(root) = args.arrangement_server {
        return arrangement::run(&root, args.threads as usize);
    }
    let request = args.request.context("A request file is required")?;
    let response = args.response.context("A response file is required")?;
    ensure!(
        request.is_absolute() && response.is_absolute(),
        "Request/response paths must be absolute"
    );
    let prepared = prepare(&request, &response)?;
    // SAFETY: this executable is still single-threaded. Configure the process
    // temp location before Rayon, codecs, or inference can start any threads.
    // Shared codec supervision uses tempfile for stderr, so its inherited temp
    // directory must obey the same command containment as explicit assets.
    unsafe {
        for key in ["TMPDIR", "TMP", "TEMP"] {
            std::env::set_var(key, &prepared.out_dir);
        }
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(args.threads as usize)
        .build_global()?;
    execute_prepared(prepared)
}

struct Prepared {
    request: request::Request,
    paths: paths::ProjectPaths,
    out_dir: PathBuf,
    response_path: PathBuf,
}

#[cfg(test)]
fn execute(request_path: &std::path::Path, response_path: &std::path::Path) -> Result<()> {
    execute_prepared(prepare(request_path, response_path)?)
}

fn prepare(request_path: &std::path::Path, response_path: &std::path::Path) -> Result<Prepared> {
    const MAX_REQUEST_BYTES: u64 = 16 * 1024 * 1024;
    let metadata = std::fs::symlink_metadata(request_path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_REQUEST_BYTES,
        "Request must be a regular JSON file of at most 16 MiB"
    );
    let mut bytes = Vec::new();
    File::open(request_path)?
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_REQUEST_BYTES,
        "Request exceeds 16 MiB"
    );
    let request: request::Request =
        serde_json::from_slice(&bytes).context("Invalid worker request")?;
    request.validate()?;
    let paths = paths::ProjectPaths::new(&request.project_root)?;
    paths.input(request_path)?;
    let out_dir = paths.directory(&request.out_dir)?;
    let response_path = paths.response(response_path, &out_dir)?;
    Ok(Prepared {
        request,
        paths,
        out_dir,
        response_path,
    })
}

fn execute_prepared(prepared: Prepared) -> Result<()> {
    let Prepared {
        request,
        paths,
        out_dir,
        response_path,
    } = prepared;
    let settings = settings::parse(&request.settings, &out_dir)?;
    progress(0.02, "Reading audio");
    let response = if matches!(
        request.operation,
        request::Operation::Separate | request::Operation::Mix | request::Operation::ImportStems
    ) {
        stems::run(&request, &paths, &out_dir, &settings)?
    } else {
        let source = paths.input(request.input.as_ref().context("Missing input")?)?;
        processing::single(&request, &source, &out_dir, &settings)?
    };
    response.validate(&request.limits, &out_dir)?;
    progress(0.98, "Publishing result");
    mastercraft::export::write_json(&response_path, &response)?;
    progress(1., "Complete");
    Ok(())
}

#[cfg(test)]
mod processing_tests;
#[cfg(test)]
mod request_tests;
