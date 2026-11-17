//! Explicit project saves and owned arrangement-project reopening.
use crate::{
    arrangement_arguments::ArrangementRevision,
    arrangement_project_arguments::LoadArrangementProject, protocol::Companion,
};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectRequest {
    pub operation: ProjectOperation,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ProjectOperation {
    SaveArrangement(ArrangementRevision),
    OpenArrangement(LoadArrangementProject),
    SaveStudio {},
}

#[tool_router(router=compact_project_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Only on an explicit project save/open request. operation.action: save_arrangement (Tracks canonical v2 sources, clips, scoped EQ and Undo/Redo; current session_id/expected_revision), open_arrangement (owned project_id; omit replacement fields only for an empty workspace, otherwise provide both replace_session_id and expected_revision), save_studio (legacy aligned/cleanup workspace only, not Tracks). Poll get_job_status for project_id/project_path or restored session metadata. No caller paths, final mix export, autoplay or desktop launch; failures preserve current work.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn project(
        &self,
        Parameters(request): Parameters<ProjectRequest>,
    ) -> Result<Json<Value>, String> {
        match request.operation {
            ProjectOperation::SaveArrangement(value) => {
                self.save_arrangement_project(Parameters(value))
            }
            ProjectOperation::OpenArrangement(value) => {
                self.load_arrangement_project(Parameters(value))
            }
            ProjectOperation::SaveStudio {} => self.save_studio_project(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{arrangement_arguments::CreateArrangement, region_edit_test_support::Workspace};
    use serde_json::json;

    #[test]
    fn project_schema_and_variants_reject_path_or_legacy_scope_smuggling() {
        let schema = serde_json::to_value(schemars::schema_for!(ProjectRequest)).unwrap();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["required"], json!(["operation"]));
        for valid in [
            json!({"operation":{"action":"save_studio"}}),
            json!({"operation":{"action":"open_arrangement","project_id":"owned-id"}}),
        ] {
            serde_json::from_value::<ProjectRequest>(valid).unwrap();
        }
        for invalid in [
            json!({"operation":{"action":"open_arrangement","path":"project.zip"}}),
            json!({"operation":{"action":"open_arrangement","project_id":"owned-id","path":"project.zip"}}),
            json!({"operation":{"action":"save_studio","session_id":"tracks-session"}}),
        ] {
            assert!(serde_json::from_value::<ProjectRequest>(invalid).is_err());
        }
    }

    #[test]
    fn stale_save_and_incomplete_replacement_fail_before_job_reservation() {
        let workspace = Workspace::new();
        let companion = Companion::new(workspace.engine.clone());
        let state = companion
            .create_arrangement(Parameters(CreateArrangement {
                sample_rate: 8000,
                channels: 2,
                replace_session_id: None,
            }))
            .unwrap()
            .0;
        for operation in [
            json!({"action":"save_arrangement","session_id":state["session_id"],"expected_revision":1}),
            json!({"action":"open_arrangement","project_id":"owned-id","replace_session_id":state["session_id"]}),
            json!({"action":"open_arrangement","project_id":"owned-id","expected_revision":0}),
        ] {
            let request = serde_json::from_value(json!({"operation":operation})).unwrap();
            assert!(companion.project(Parameters(request)).is_err());
        }
        let store = workspace.engine.lock().unwrap();
        assert!(store.jobs.is_empty() && store.active.is_none());
        assert_eq!(
            store.arrangements.current.as_ref().unwrap().epoch(),
            state["session_id"].as_str().unwrap()
        );
    }
}
