//! Audio operations shared by the protocol tools; jobs commit session state on success.
use crate::{
    audio_input,
    jobs::{Change, ContextData, Product},
    options::{self, EqOptions, ExportFormat, MasterOptions},
    state::{Input, Master, Reference, analysis_summary},
};
use anyhow::{Context, Result};
use mastercraft::{
    analysis, export, preview,
    session_document::{SessionDocument, SessionSource},
    wav_encoding,
};
use serde_json::json;
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

pub fn load(ctx: ContextData, path: PathBuf, reference: bool) -> Result<Product> {
    ctx.progress(
        0.05,
        "Decoding local audio within the configured input budget",
    );
    let (audio, identity) = audio_input::load(
        &path,
        ctx.engine.max_decoded_bytes,
        ctx.engine.decode_timeout,
        ctx.cancel.clone(),
    )?;
    ctx.check()?;
    let source = SessionSource { path, identity };
    if reference {
        let profile = options::profile(&ctx.snapshot.settings.profile)?;
        ctx.progress(0.4, "Measuring the reference track");
        let measured = analysis::analyze(&audio, &profile)?;
        ctx.check()?;
        Ok(Product {
            result: json!({"reference_loaded":true,"analysis":analysis_summary(&measured)}),
            change: Change::Reference(Some(Arc::new(Reference {
                source,
                analysis: measured,
            }))),
            committed: false,
        })
    } else {
        let id = format!("source-{}", ctx.id);
        let result = json!({"source_id":id,"file":ctx.engine.locator(&source.path),"duration_seconds":audio.duration(),"sample_rate":audio.sample_rate,"channels":audio.channels});
        Ok(Product {
            result,
            change: Change::Source(Arc::new(Input {
                id,
                source,
                audio: Arc::new(audio),
                generated: None,
                studio_preview: None,
            })),
            committed: false,
        })
    }
}

pub fn analyze(ctx: ContextData, source_id: String, suggest: bool) -> Result<Product> {
    let source = ctx.snapshot.source(&source_id)?;
    let profile = options::profile(&ctx.snapshot.settings.profile)?;
    ctx.progress(0.1, "Measuring the track");
    let measured = if suggest {
        ctx.snapshot
            .master
            .as_ref()
            .map(|m| m.preview.raw_analysis.as_ref().clone())
    } else {
        None
    };
    let measured = match measured {
        Some(value) => value,
        None => analysis::analyze(&source.audio, &profile)?,
    };
    ctx.check()?;
    let result = if suggest {
        json!({"bands":mastercraft::dsp::eq_suggestion::suggest(&measured, &profile)?,"applied":false,
            "basis":if ctx.snapshot.master.is_some() {"master_before_manual_eq"} else {"original"},
            "note":"Deterministic, conservative tonal suggestions. Applying them before listening is a provisional measurement-based edit, not an auditioned approval. A profile is not a quality score."})
    } else {
        analysis_summary(&measured)
    };
    Ok(Product::value(result))
}

fn prepared(
    ctx: &ContextData,
    prepared: preview::Preview,
    settings: mastercraft::config::Settings,
) -> Result<Product> {
    ctx.check()?;
    ctx.engine.policy.check_output()?;
    ctx.progress(0.9, "Writing and verifying the temporary EQ preview");
    let mut file = tempfile::Builder::new()
        .prefix("preview-")
        .suffix(".wav")
        .tempfile_in(ctx.engine.cache.path())?;
    wav_encoding::write_wav(&prepared.audio, file.as_file_mut(), "wav32f", false)?;
    file.flush()?;
    file.as_file().sync_all()?;
    // Close the file handle before exposing it, retaining automatic path cleanup.
    // In particular, Windows directory listings must see the finished file size.
    let file = file.into_temp_path();
    ctx.check()?;
    let id = format!("preview-{}", ctx.id);
    let listening = if ctx.engine.desktop.is_some() {
        "Use open_in_desktop so the user can listen and refine the result."
    } else {
        "No desktop executable is configured. The user can play the relative_preview_path WAV with a local audio player. It is temporary; do not present it as a public link or claim to have heard it. Owner can configure a matching desktop build and reconnect."
    };
    let result = json!({"preview_id":id,"relative_preview_path":ctx.engine.policy.relative_output(&file)?,
        "preview_bytes":std::fs::metadata(&file)?.len(),"measurements":prepared.report.output.metrics,"notes":prepared.report.notes,"target_lufs":prepared.report.target_lufs,
        "listening":listening,"next":"Adjust EQ or re-master if needed, then call export_audio only on the user's request. Temporary preview is removed when replaced or the companion exits."});
    Ok(Product {
        result,
        change: Change::Master(Arc::new(Master {
            id,
            preview: prepared,
            settings,
            file: Arc::new(file),
        })),
        committed: false,
    })
}

pub fn master(ctx: ContextData, source_id: String, options: MasterOptions) -> Result<Product> {
    let source = ctx.snapshot.source(&source_id)?;
    let settings = options.apply(&ctx.snapshot.settings, ctx.snapshot.reference.is_some())?;
    let profile = options::profile(&settings.profile)?;
    let prepared_audio = preview::master(
        &source.audio,
        &settings,
        &profile,
        ctx.snapshot.reference.as_ref().map(|r| &r.analysis),
        &source
            .source
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        &|fraction, message| {
            ctx.progress(
                fraction * 0.8,
                if fraction >= 1. {
                    "Automatic processing finished; preparing the manual EQ preview"
                } else {
                    message
                },
            )
        },
        &|| ctx.cancelled(),
    )?;
    prepared(&ctx, prepared_audio, settings)
}

pub fn equalize(ctx: ContextData, preview_id: String, options: EqOptions) -> Result<Product> {
    let current = ctx.snapshot.master(&preview_id)?;
    let settings = options.apply(&current.settings)?;
    let profile = options::profile(&settings.profile)?;
    ctx.progress(0.1, "Rendering EQ from the original pre-EQ master");
    let (audio, parameters, report) = preview::resolve_eq(
        &current.preview.raw,
        &settings,
        &profile,
        &current.preview.report,
        &|| ctx.cancelled(),
    )?;
    prepared(
        &ctx,
        preview::Preview {
            raw: current.preview.raw.clone(),
            raw_analysis: current.preview.raw_analysis.clone(),
            audio: Arc::new(audio),
            parameters,
            report,
        },
        settings,
    )
}

pub fn export_audio(
    ctx: ContextData,
    preview_id: String,
    stem: String,
    format: ExportFormat,
    sample_rate: u32,
    dither: bool,
) -> Result<Product> {
    let current = ctx.snapshot.master(&preview_id)?;
    ctx.engine.policy.check_output()?;
    crate::file_policy::export_stem(&stem)?;
    let mut settings = current.settings.clone();
    settings.out_dir = ctx.engine.policy.output.clone();
    settings.formats = vec![format.name().into()];
    settings.suffix = String::new();
    settings.output_sr = sample_rate;
    settings.dither = dither;
    settings.validate()?;
    ctx.progress(
        0.05,
        "Encoding and verifying a new export without replacing existing files",
    );
    let mut exports = export::export(
        &current.preview.audio,
        Path::new(&format!("{stem}.wav")),
        &settings,
        &|| ctx.cancelled(),
    )?;
    // From this point an audio file is durable. Report it even if cancellation arrives late.
    let mut report = current.preview.report.clone();
    report.configuration = crate::state::public_settings(&settings);
    for item in &mut exports {
        let path = item["path"].as_str().context("Export returned no path")?;
        item["path"] = ctx.engine.policy.relative_output(Path::new(path))?.into();
    }
    report.exports = exports.clone();
    let report_result = (|| -> Result<String> {
        let mut file = tempfile::NamedTempFile::new_in(&settings.out_dir)?;
        serde_json::to_writer_pretty(&mut file, &report)?;
        file.flush()?;
        file.as_file().sync_all()?;
        let path =
            export::persist_unique(file, &settings.out_dir, &format!("{stem}_report"), "json")?;
        ctx.engine.policy.relative_output(&path)
    })();
    let (report_path, warning) = match report_result {
        Ok(path) => (Some(path), None),
        Err(e) => (
            None,
            Some(ctx.engine.policy.redact(&format!(
                "Audio saved, but report could not be saved: {e:#}"
            ))),
        ),
    };
    Ok(Product {
        result: json!({"exports":exports,"report_path":report_path,"warning":warning}),
        change: Change::None,
        committed: true,
    })
}

pub fn desktop(ctx: ContextData) -> Result<Product> {
    let source = ctx.snapshot.source.as_ref().context("Load audio first")?;
    let executable = ctx
        .engine
        .desktop
        .as_ref()
        .context("Configure --desktop-exe with a desktop build that supports --session")?;
    ctx.progress(0.1, "Verifying source files before desktop handoff");
    let handoff_source = if source.generated.is_some() {
        let mut file = tempfile::NamedTempFile::new_in(&ctx.engine.policy.output)?;
        std::io::copy(&mut std::fs::File::open(&source.source.path)?, &mut file)?;
        file.flush()?;
        file.as_file().sync_all()?;
        let path = export::persist_unique(
            file,
            &ctx.engine.policy.output,
            "Studio-mastering-source",
            "wav",
        )?;
        SessionSource {
            path,
            identity: source.source.identity.clone(),
        }
    } else {
        source.source.clone()
    };
    let document = SessionDocument::new(
        handoff_source,
        ctx.snapshot.reference.as_ref().map(|r| r.source.clone()),
        ctx.snapshot.settings.clone(),
    );
    document.validate()?;
    ctx.engine.policy.check_output()?;
    let mut file = tempfile::NamedTempFile::new_in(&ctx.engine.policy.output)?;
    serde_json::to_writer_pretty(&mut file, &document)?;
    file.flush()?;
    file.as_file().sync_all()?;
    ctx.check()?;
    let path = export::persist_unique(
        file,
        &ctx.engine.policy.output,
        "desktop_session",
        "slimeybeats.json",
    )?;
    let mut command = std::process::Command::new(executable);
    command.arg("--session").arg(&path);
    if ctx.snapshot.master.is_some() {
        command.args(["--auto-master", "--view", "eq"]);
    }
    let launch = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let (process_id, warning) = match launch {
        Ok(mut child) => {
            let id = child.id();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            (Some(id), None)
        }
        Err(error) => (
            None,
            Some(format!("Session saved, but desktop launch failed: {error}")),
        ),
    };
    Ok(Product {
        result: json!({"session_path":ctx.engine.policy.relative_output(&path)?,"desktop_launched":warning.is_none(),"process_id":process_id,"warning":warning,
        "note":"The desktop rebuilds the preview with these settings. This is a new desktop session; later AI and desktop edits are independent. No final audio was exported."}),
        change: Change::None,
        committed: true,
    })
}
