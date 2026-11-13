use crate::{
    jobs::{Change, ContextData, Product},
    protocol::Companion,
};
use anyhow::{Context, Result, ensure};
use mastercraft::studio_native::session::{self, StudioSnapshot};
use rmcp::{Json, tool, tool_router};
use serde_json::{Value, json};

fn snapshot(ctx: &ContextData) -> Result<StudioSnapshot> {
    let studio = &ctx.snapshot.studio;
    let preview = studio
        .current
        .as_ref()
        .context("Prepare a Studio preview first")?;
    let source = studio
        .bank
        .as_ref()
        .map(|b| b.original_mix.clone())
        .or_else(|| ctx.snapshot.source.as_ref().map(|s| s.audio.clone()))
        .unwrap_or_else(|| preview.before.clone());
    let name = ctx
        .snapshot
        .source
        .as_ref()
        .and_then(|s| s.source.path.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Imported stem mix".into());
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
        source_name: name,
        source,
        stems: studio.bank.as_ref().map(|b| b.stems.clone()),
        mix: studio
            .bank
            .as_ref()
            .map(|b| b.controls.clone())
            .unwrap_or_default(),
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
    if launch {
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
