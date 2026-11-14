//! Opt-in actual model smoke test. Model/runtime dependencies are installed separately.
use anyhow::Result;
use clap::Parser;
use mastercraft::{
    audio,
    studio::stem_mix,
    studio_native::{inference::SeparationMode, runtime::StemRuntime},
};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    python: PathBuf,
    #[arg(long)]
    model: PathBuf,
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    out: PathBuf,
    #[arg(long)]
    two_stems: bool,
}
fn main() -> Result<()> {
    let args = Args::parse();
    std::fs::create_dir_all(&args.out)?;
    let input = audio::load(&args.input)?;
    let runtime = StemRuntime::configured(args.python, args.model);
    let start = Instant::now();
    let separated = runtime.separate(
        &input,
        if args.two_stems {
            SeparationMode::VocalInstrumental
        } else {
            SeparationMode::FourStems
        },
        &args.out,
        &|fraction, message| eprintln!("{fraction:.2}: {message}"),
        Arc::new(AtomicBool::new(false)),
    )?;
    let mut settings = separated.stems.defaults();
    settings.protect_peaks = false;
    let recombined = stem_mix::render(&separated.stems, &settings, &|| false)?;
    let error = (input
        .samples
        .iter()
        .zip(&recombined.samples)
        .map(|(&a, &b)| ((a - b) as f64).powi(2))
        .sum::<f64>()
        / input.samples.len() as f64)
        .sqrt();
    let mut stems = Vec::new();
    for stem in &separated.stems.tracks {
        let path = args.out.join(format!("{}.wav", stem.kind.name()));
        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(path)?;
        mastercraft::wav_encoding::write_wav(&stem.audio, file, "wav32f", false)?;
        stems.push(json!({"kind":stem.kind,"frames":stem.audio.frames(),"channels":stem.audio.channels,"sample_rate":stem.audio.sample_rate,"rms":stem.audio.rms()}));
    }
    let report = json!({"stems":stems,"recombination_error_rms":error,"elapsed_seconds":start.elapsed().as_secs_f64(),"model":separated.metadata});
    mastercraft::export::write_json(&args.out.join("stem-probe.json"), &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
