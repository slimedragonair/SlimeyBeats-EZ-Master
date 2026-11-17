//! Canonical project jobs: persistence never requires a rendered whole-song preview.
use crate::{
    arrangement_project_arguments::LoadArrangementProject,
    arrangement_project_files::{self as files, SavedProject},
    arrangement_state::ArrangementWorkspace,
    jobs::{Change, ContextData, Product},
    state::{Engine, Store},
};
use anyhow::{Result, ensure};
use mastercraft::{
    studio::arrangement_session::{ArrangementSession, SessionLimits},
    studio_native::arrangement_project::{self as native, ProjectLimits},
};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct SelectionGuard(pub Option<(String, u64)>);
impl SelectionGuard {
    pub fn selected(id: &str, revision: u64) -> Self {
        Self(Some((id.into(), revision)))
    }
    pub fn for_load(request: &LoadArrangementProject) -> Result<Self> {
        ensure!(
            request.replace_session_id.is_some() == request.expected_revision.is_some(),
            "Supply both replace_session_id and expected_revision, or omit both for an empty workspace"
        );
        Ok(Self(
            request
                .replace_session_id
                .clone()
                .zip(request.expected_revision),
        ))
    }
    pub fn check(&self, store: &Store) -> Result<()> {
        match (&self.0, &store.arrangements.current) {
            (Some((id, revision)), Some(_)) => {
                store.arrangements.selected(id, *revision)?;
            }
            (None, None) => {}
            _ => anyhow::bail!(
                "Arrangement changed; use get_arrangement and supply its current replace_session_id and expected_revision"
            ),
        }
        Ok(())
    }
}

pub struct LoadedProject {
    pub(crate) guard: SelectionGuard,
    pub(crate) workspace: ArrangementWorkspace,
}

pub fn limits(engine: &Engine) -> ProjectLimits {
    ProjectLimits {
        max_asset_bytes: engine.max_decoded_bytes,
        max_total_bytes: (engine.max_decoded_bytes as u64)
            .saturating_mul(8)
            .min(4 * 1024 * 1024 * 1024),
        max_output_bytes: ProjectLimits::default().max_output_bytes,
    }
}

fn result(engine: &Engine, saved: &SavedProject, session: &ArrangementSession) -> Result<Value> {
    Ok(
        json!({"project_id":saved.id,"project_path":engine.policy.relative_output(&saved.manifest)?,
        "manifest_identity":saved.identity,"format":"slimeybeats-studio","version":2,
        "session_id":session.epoch(),"revision":session.revision(),
        "note":"Self-contained immutable arrangement project, not a rendered mix or final audio export. Retain project_id to reopen with the same configured output root after restarting. Original library files are not required."}),
    )
}

pub fn save(ctx: ContextData, session: Arc<ArrangementSession>, launch: bool) -> Result<Product> {
    ctx.check()?;
    let desktop_capabilities = if launch {
        let executable = ctx.engine.desktop.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Configure a matching preview desktop executable first")
        })?;
        ctx.progress(
            0.01,
            "Verifying desktop v2 project capabilities before saving",
        );
        Some(crate::arrangement_desktop::require_v2(
            executable,
            ctx.cancel.clone(),
        )?)
    } else {
        None
    };
    ctx.check()?;
    let snapshot = session.project_snapshot()?;
    let reservation = files::Reservation::new(&ctx.engine)?;
    ctx.progress(
        0.05,
        "Saving canonical v2 sources, clips, scoped EQ and history; no mix render",
    );
    let manifest =
        native::save_with_limits(&snapshot, reservation.path(), limits(&ctx.engine), &|| {
            ctx.cancelled()
        })?;
    let saved = {
        let store = ctx.engine.lock()?;
        ctx.check()?;
        store
            .arrangements
            .selected(session.epoch(), session.revision())?;
        reservation.publish(&ctx.engine, &manifest)?
    };
    let mut value = result(&ctx.engine, &saved, &session)?;
    value["desktop_launched"] = false.into();
    value["process_id"] = Value::Null;
    value["warning"] = Value::Null;
    if let Some(capabilities) = desktop_capabilities {
        value["desktop_capabilities"] = capabilities;
    }
    if launch {
        // Saving has committed. A late cancellation/stale selection must never launch an app.
        let store = ctx.engine.lock()?;
        if ctx.cancelled()
            || store
                .arrangements
                .selected(session.epoch(), session.revision())
                .is_err()
        {
            value["warning"] = "Project saved; desktop launch cancelled because the request or arrangement changed".into();
        } else {
            let child = std::process::Command::new(ctx.engine.desktop.as_ref().unwrap())
                .arg("--studio-session")
                .arg(&saved.manifest)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            match child {
                Ok(mut child) => {
                    value["desktop_launched"] = true.into();
                    value["process_id"] = child.id().into();
                    std::thread::spawn(move || {
                        let _ = child.wait();
                    });
                }
                Err(error) => {
                    value["warning"] =
                        format!("Project saved, but desktop launch failed: {error}").into()
                }
            }
        }
        value["desktop_note"] = "Desktop receives --studio-session with the canonical v2 project; later desktop/MCP edits are independent. This tool does not start playback.".into();
    }
    Ok(Product {
        result: value,
        change: Change::None,
        committed: true,
    })
}

pub fn load(ctx: ContextData, request: LoadArrangementProject) -> Result<Product> {
    let guard = SelectionGuard::for_load(&request)?;
    ctx.check()?;
    {
        let store = ctx.engine.lock()?;
        guard.check(&store)?;
    }
    let saved = files::resolve(&ctx.engine, &request.project_id)?;
    ctx.progress(
        0.05,
        "Verifying project hashes and loading immutable sources; no mix render or playback",
    );
    let snapshot =
        native::load_with_limits(&saved.manifest, limits(&ctx.engine), &|| ctx.cancelled())?;
    let checked = files::resolve(&ctx.engine, &request.project_id)?;
    ensure!(
        checked.identity == saved.identity,
        "Project changed while loading"
    );
    ctx.check()?;
    let next_revision = guard
        .0
        .as_ref()
        .map_or(0, |(_, revision)| revision.saturating_add(1));
    let session = Arc::new(ArrangementSession::from_project_at_revision(
        snapshot,
        format!("arr-{}", ctx.id),
        SessionLimits {
            max_decoded_bytes: limits(&ctx.engine).max_total_bytes,
            ..Default::default()
        },
        next_revision,
    )?);
    let workspace = ArrangementWorkspace {
        current: Some(session),
        preview: None,
    };
    let mut value = workspace.summary(&ctx.engine)?;
    value["project_id"] = saved.id.into();
    value["note"] = "Loaded metadata, immutable sources and retained history only. No preview rendered, desktop launched, playback started, master or legacy Studio bank replaced. Use the returned session_id/revision for subsequent actions.".into();
    Ok(Product {
        result: value,
        change: Change::ArrangementProject(Box::new(LoadedProject { guard, workspace })),
        committed: false,
    })
}
