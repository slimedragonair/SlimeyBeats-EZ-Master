use anyhow::Result;
use clap::Parser;
use mastercraft::studio_native::runtime::RuntimePaths;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    data: PathBuf,
}
fn main() -> Result<()> {
    let args = Args::parse();
    let paths = RuntimePaths { base: args.data };
    let result = paths.install(
        &|fraction, message| eprintln!("{fraction:.2}: {message}"),
        Arc::new(AtomicBool::new(false)),
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
