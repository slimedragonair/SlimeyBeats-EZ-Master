//! Three explicit persistence/handoff tools, independent of legacy Studio previews.
use crate::{
    arrangement_arguments::ArrangementRevision,
    arrangement_project_arguments::LoadArrangementProject,
    arrangement_project_operations::{self as operations, SelectionGuard},
    protocol::Companion,
};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use serde_json::Value;

#[tool_router(router=arrangement_project_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Explicitly save the current arrangement as a new self-contained canonical v2 project, preserving sources, clips, scoped EQ and Undo/Redo. Requires current session_id/expected_revision. No preview or whole-song render required. Poll for project_id; retain it across restarts under the same configured output root. No caller paths, overwrite, master or legacy-bank changes.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn save_arrangement_project(
        &self,
        Parameters(a): Parameters<ArrangementRevision>,
    ) -> Result<Json<Value>, String> {
        self.arrangement_project_save(a, false)
    }

    #[tool(
        description = "Explicitly reopen an owned arrangement project_id saved under this configured output root. Fresh/empty workspace: omit replacement fields. Existing workspace: supply both current replace_session_id and expected_revision from get_arrangement. Poll for new session_id/revision/assets/history. Verifies v2 hashes, containment and budgets before atomic adoption. Failure/cancellation preserves prior state. No whole-song render, autoplay, desktop launch, master or legacy-bank replacement; paths/foreign IDs are rejected.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn load_arrangement_project(
        &self,
        Parameters(a): Parameters<LoadArrangementProject>,
    ) -> Result<Json<Value>, String> {
        self.response((|| {
            let guard = SelectionGuard::for_load(&a)?;
            self.engine.start_checked(
                "load_arrangement_project",
                move |store| guard.check(store),
                move |ctx| operations::load(ctx, a),
            )
        })())
    }

    #[tool(
        description = "Only on an explicit request, probe the configured desktop's --studio-project-capabilities with bounded output/time, require canonical v2 live editing and paused opening, then save a new v2 project and launch with --studio-session. Requires current session_id/expected_revision and desktop_configured:true; configuration or matching version/build labels alone do not establish compatibility. Incompatible/invalid/timed-out probes fail before creating project folders. Poll for project_id, desktop_capabilities, desktop_launched and warning. No final mix export/autoplay; desktop/MCP edits remain independent. No caller executable/project paths.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn open_arrangement_in_desktop(
        &self,
        Parameters(a): Parameters<ArrangementRevision>,
    ) -> Result<Json<Value>, String> {
        self.arrangement_project_save(a, true)
    }
}

impl Companion {
    fn arrangement_project_save(
        &self,
        a: ArrangementRevision,
        launch: bool,
    ) -> Result<Json<Value>, String> {
        self.response((|| {
            if launch {
                anyhow::ensure!(
                    self.engine.desktop.is_some(),
                    "Configure a matching desktop executable first"
                );
            }
            let session = self
                .engine
                .lock()?
                .arrangements
                .selected(&a.session_id, a.expected_revision)?;
            let guard = SelectionGuard::selected(&a.session_id, a.expected_revision);
            self.engine.start_checked(
                if launch {
                    "open_arrangement_in_desktop"
                } else {
                    "save_arrangement_project"
                },
                move |store| guard.check(store),
                move |ctx| operations::save(ctx, session, launch),
            )
        })())
    }
}

#[cfg(test)]
#[path = "arrangement_project_tests.rs"]
mod tests;
