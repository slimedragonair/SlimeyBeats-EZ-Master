use anyhow::{Context, Result, ensure};
use clap::Parser;
use mastercraft::{analysis, audio, config::Settings, engine, export, profiles};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Instant,
};

#[path = "../cli_arguments.rs"]
mod cli_arguments;
use cli_arguments::Arguments;
fn main() {
    if let Err(e) = run() {
        eprintln!("ERROR: {e:#}");
        std::process::exit(2);
    }
}
fn run() -> Result<()> {
    let a = Arguments::parse();
    if let Some(n) = a.threads {
        ensure!(n > 0, "Threads must be positive");
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()?;
    }
    let root = mastercraft::paths::app_root();
    if a.list_profiles {
        for (k, p) in profiles::builtin() {
            println!("{k:24} {}", p["display_name"].as_str().unwrap_or(""));
        }
        return Ok(());
    }
    let mut s = if let Some(path) = &a.settings {
        Settings::load(path)?
    } else {
        Settings::default()
    };
    if let Some(p) = a.profile {
        s.profile = p;
    }
    if let Some(m) = a.mode {
        s.mode = m;
    }
    if let Some(v) = a.lufs {
        s.target_lufs = Some(v);
    }
    if let Some(v) = a.ceiling {
        s.ceiling_dbtp = v;
    }
    if a.spotify_strict && s.mode == "loud" {
        s.ceiling_dbtp = -2.;
    }
    if let Some(v) = a.intensity {
        s.intensity = v;
    }
    if !a.format.is_empty() {
        s.formats = a.format;
    }
    if let Some(v) = a.out {
        s.out_dir = v;
    }
    if let Some(v) = a.sr {
        s.output_sr = v;
    }
    if let Some(v) = a.suffix {
        s.suffix = v;
    }
    if let Some(v) = a.reference {
        s.reference = Some(v);
    }
    if let Some(v) = a.match_strength {
        s.match_strength = v;
    }
    s.match_loudness |= a.match_loudness;
    s.match_width &= !a.no_match_width;
    s.safety &= !a.no_safety;
    s.dither &= !a.no_dither;
    s.disabled.extend(a.disable);
    if let Some(path) = a.eq_preset {
        s.import_eq_preset(&path)?;
    }
    s.validate()?;
    let mut files = BTreeSet::new();
    let mut missing = Vec::new();
    for input in &a.inputs {
        let path = Path::new(input);
        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let p = entry?.path();
                if audio::supported(&p) {
                    files.insert(p);
                }
            }
        } else if path.is_file() {
            files.insert(path.to_path_buf());
        } else {
            let mut found = false;
            for entry in glob::glob(input)? {
                let p = entry?;
                if p.is_file() && audio::supported(&p) {
                    files.insert(p);
                    found = true;
                }
            }
            if !found {
                missing.push(input.clone());
            }
        }
    }
    ensure!(
        !files.is_empty() || !missing.is_empty(),
        "Add audio paths, a folder, or a glob; --help lists options"
    );
    if a.calibrate {
        ensure!(missing.is_empty(), "Missing reference files: {missing:?}");
        let name = a.name.context("--calibrate requires --name")?;
        ensure!(
            !name.is_empty() && !name.contains(['/', '\\', ':']) && !name.contains(".."),
            "Invalid profile name"
        );
        let mut p = profiles::resolve(&a.base, &root.join("profiles"))?;
        let curves: Vec<_> = files
            .iter()
            .map(|f| Ok(analysis::analyze(&audio::load(f)?, &p)?.measured_db))
            .collect::<Result<_>>()?;
        p["tonal_db"] = serde_json::json!(
            (0..10)
                .map(|i| mastercraft::meters::percentile(
                    &curves.iter().map(|c| c[i]).collect::<Vec<_>>(),
                    0.5
                ))
                .collect::<Vec<_>>()
        );
        p["display_name"] = serde_json::json!(name);
        let path = root.join("profiles").join(format!("{name}.json"));
        export::write_json(&path, &p)?;
        println!("Saved {}", path.display());
        return Ok(());
    }
    let profile = profiles::resolve(&s.profile, &root.join("profiles"))?;
    let reference = s
        .reference
        .as_ref()
        .map(|p| -> Result<_> { analysis::analyze(&audio::load(p)?, &profile) })
        .transpose()?;
    let mut failures = missing.len();
    for name in &missing {
        eprintln!("FAILED: input not found: {name}");
    }
    let total = files.len() + missing.len();
    for file in files {
        let result = (|| -> Result<()> {
            let start = Instant::now();
            let input = audio::load(&file)?;
            if a.analyze_only {
                let report = analysis::analyze(&input, &profile)?;
                println!(
                    "{}  {:.2} LUFS  {:.2} dBTP",
                    file.display(),
                    report.metrics.integrated_lufs,
                    report.metrics.true_peak_dbtp
                );
                if let Some(path) = &a.report_json {
                    let p = if total == 1 && path.extension().is_some_and(|e| e == "json") {
                        path.clone()
                    } else {
                        path.join(format!(
                            "{}_analysis.json",
                            file.file_stem().unwrap().to_string_lossy()
                        ))
                    };
                    export::write_json(&p, &report)?;
                }
                return Ok(());
            }
            let runs = a.benchmark.max(1);
            let mut times = Vec::new();
            let mut final_master = None;
            for _ in 0..runs {
                let t = Instant::now();
                let master = engine::master(
                    &input,
                    &s,
                    &profile,
                    reference.as_ref(),
                    &file.to_string_lossy(),
                    &|_, _| {},
                    &|| false,
                )?;
                times.push(t.elapsed().as_secs_f64());
                final_master = Some(master);
            }
            let mut mastered = final_master.unwrap();
            if a.benchmark > 0 {
                println!(
                    "{}",
                    serde_json::to_string(
                        &serde_json::json!({"file":file,"seconds":times,"audio_seconds":input.duration(),"report":mastered.report})
                    )?
                );
                return Ok(());
            }
            mastered.report.exports = export::export(&mastered.audio, &file, &s, &|| false)?;
            if !a.no_report {
                let first = PathBuf::from(mastered.report.exports[0]["path"].as_str().unwrap());
                let report_path = if let Some(path) = &a.report_json {
                    if total == 1 && path.extension().is_some_and(|e| e == "json") {
                        path.clone()
                    } else {
                        path.join(format!(
                            "{}.report.json",
                            first.file_stem().unwrap().to_string_lossy()
                        ))
                    }
                } else {
                    first.with_extension("report.json")
                };
                export::write_json(&report_path, &mastered.report)?;
            }
            println!(
                "{} -> {:.2} LUFS / {:.2} dBTP, {:.2}s total",
                file.display(),
                mastered.report.output.metrics.integrated_lufs,
                mastered.report.output.metrics.true_peak_dbtp,
                start.elapsed().as_secs_f64()
            );
            for e in &mastered.report.exports {
                println!("  {}", e["path"].as_str().unwrap());
            }
            Ok(())
        })();
        if let Err(error) = result {
            failures += 1;
            eprintln!("FAILED {}: {error:#}", file.display());
        }
    }
    if failures > 0 {
        eprintln!("{} OK, {failures} failed", total - failures);
        std::process::exit(if failures == total { 3 } else { 1 });
    }
    Ok(())
}
