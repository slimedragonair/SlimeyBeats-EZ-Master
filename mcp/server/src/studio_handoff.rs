use crate::{
    jobs::{Change, ContextData, Product},
    protocol::Companion,
};
use anyhow::{Context, Result, ensure};
use mastercraft::studio_native::session::{self, StudioSnapshot};
use rmcp::{Json, tool, tool_router};
use serde_json::{Value, json};
use std::sync::Arc;

#[cfg(test)]
#[path = "studio_handoff_tests.rs"]
mod tests;

fn snapshot(ctx: &ContextData) -> Result<StudioSnapshot> {
    let studio = &ctx.snapshot.studio;
    let preview = studio
        .current
        .as_ref()
        .context("Prepare a Studio preview first")?;
    let source = &preview.origin_snapshot.audio;
    let bank = preview.bank_snapshot.as_ref();
    let aligned = |audio: &mastercraft::audio::Audio| {
        audio.sample_rate == preview.audio.sample_rate
            && audio.channels == preview.audio.channels
            && audio.frames() == preview.audio.frames()
    };
    ensure!(aligned(source), "Studio source and preview are not aligned");
    ensure!(
        aligned(&preview.before),
        "Studio comparison assets are not aligned"
    );
    if let Some(bank) = bank {
        ensure!(
            Arc::ptr_eq(source, &bank.original_mix),
            "Studio origin does not match its stem bank"
        );
        ensure!(
            bank.stems.tracks.iter().all(|stem| aligned(&stem.audio)),
            "Studio stem bank is not aligned with its source"
        );
        bank.controls.validate(&bank.stems)?;
    }
    let last = preview
        .steps
        .last()
        .and_then(|s| s["operation"].as_str())
        .unwrap_or("stem_mix");
    let view = match last {
        "voice_cleanup" => "voice",
        "spectral_repair" => "repair",
        "vst3" => "plugins",
        _ => "stems",
    };
    Ok(StudioSnapshot {
        source_name: preview.origin_snapshot.label.clone(),
        source: source.clone(),
        stems: bank.map(|b| b.stems.clone()),
        mix: bank.map(|b| b.controls.clone()).unwrap_or_default(),
        before: preview.before.clone(),
        preview: preview.audio.clone(),
        label: preview.label.clone(),
        notes: preview.notes.clone(),
        view: view.into(),
    })
}
fn save(ctx: ContextData, launch: bool) -> Result<Product> {
    let snapshot = snapshot(&ctx)?;
    ctx.engine.policy.check_output()?;
    ctx.progress(
        0.05,
        "Saving self-contained Studio working copies and source hashes",
    );
    let path = session::save(&snapshot, &ctx.engine.policy.output, &|| ctx.cancelled())?;
    let mut warning = None;
    let mut process_id = None;
    if launch && ctx.cancelled() {
        warning = Some("Project saved; desktop launch was cancelled".into());
    }
    if launch && !ctx.cancelled() {
        let executable = ctx
            .engine
            .desktop
            .as_ref()
            .context("Desktop executable is not configured")?;
        let child = std::process::Command::new(executable)
            .arg("--studio-session")
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match child {
            Ok(mut child) => {
                process_id = Some(child.id());
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(error) => {
                warning = Some(format!(
                    "Project saved, but the desktop could not launch: {error}"
                ))
            }
        }
    }
    Ok(Product {
        result: json!({"project_path":ctx.engine.policy.relative_output(&path)?,"desktop_launched":process_id.is_some(),"process_id":process_id,"warning":warning,"note":"This saves a self-contained editing project, not a final mastered export. Desktop and AI edits are independent after handoff. Retained working copies may use substantial disk space."}),
        change: Change::None,
        committed: true,
    })
}

#[tool_router(router=studio_handoff_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Explicitly save a self-contained Studio project containing source/stems/before/preview working WAVs, mix settings and hashes in a new folder. It can be reopened without the model runtime or original library files. This persists working audio, not a final mastered export. Poll for project_path.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn save_studio_project(&self) -> Result<Json<Value>, String> {
        self.response(self.engine.start(
            "save_studio_project",
            |s| {
                ensure!(s.studio.current.is_some(), "Prepare a Studio preview first");
                Ok(())
            },
            |ctx| save(ctx, false),
        ))
    }
    #[tool(
        description = "Open the current Studio workspace in the matching desktop app for listening/refinement. Saves a self-contained project with verified source/stem/preview WAVs; no model is needed to reopen those stems. No final mastered export. Poll. Later desktop/AI edits are independent.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn open_studio_in_desktop(&self) -> Result<Json<Value>, String> {
        self.response(self.engine.start(
            "open_studio_in_desktop",
            |s| {
                ensure!(s.studio.current.is_some(), "Prepare a Studio preview first");
                ensure!(
                    self.engine.desktop.is_some(),
                    "Configure a matching desktop executable first"
                );
                Ok(())
            },
            |ctx| save(ctx, true),
        ))
    }
}
