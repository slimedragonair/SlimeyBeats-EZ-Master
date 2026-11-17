//! Separate explicit audio exports from desktop audition and project handoff.
use crate::{
    arrangement_arguments::{ArrangementRevision, ExportArrangement},
    protocol::{Companion, ExportRequest},
    stem_protocol::BankId,
    studio_protocol::StudioExportRequest,
};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExportResultRequest {
    pub operation: ExportResultOperation,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ExportResultOperation {
    Mastering(ExportRequest),
    Studio(StudioExportRequest),
    Arrangement(ExportArrangement),
    RawStems(BankId),
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OpenDesktopRequest {
    pub operation: OpenDesktopOperation,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum OpenDesktopOperation {
    Mastering {},
    Studio {},
    Arrangement(ArrangementRevision),
}

#[tool_router(router=compact_delivery_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Export only after an explicit owner export/save-audio request. operation.action: mastering (current master preview_id), studio (current legacy studio_preview_id, not Tracks), arrangement (Tracks session_id/expected_revision), raw_stems (owned stem_set_id; separate raw files, not a mix). The first three require a new filename stem name without path/extension; format is wav16/wav24/wav32f/flac16/flac24/mp3, sample_rate in Hz with 0 retaining the source rate. Codec/resampling support is required where applicable. Format/bit-depth or collision suffixes may be added; use the completed job returned paths as actual filenames. Poll get_job_status. Preserves sources and existing exports; never infers a target from filenames.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn export_result(
        &self,
        Parameters(request): Parameters<ExportResultRequest>,
    ) -> Result<Json<Value>, String> {
        match request.operation {
            ExportResultOperation::Mastering(value) => self.export_audio(Parameters(value)),
            ExportResultOperation::Studio(value) => self.export_studio_audio(Parameters(value)),
            ExportResultOperation::Arrangement(value) => self.export_arrangement(Parameters(value)),
            ExportResultOperation::RawStems(value) => self.export_stems(Parameters(value)),
        }
    }

    #[tool(
        description = "Only on an explicit request to open/listen in the configured desktop. operation.action: mastering (current mastering session), studio (legacy aligned/cleanup session, not Tracks), arrangement (Tracks current session_id/expected_revision; verifies desktop canonical-v2 support before saving/launching). Poll get_job_status for handoff details. Does not export a final mix or autoplay; later desktop and MCP edits are independent. No caller executable or project paths.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn open_desktop(
        &self,
        Parameters(request): Parameters<OpenDesktopRequest>,
    ) -> Result<Json<Value>, String> {
        match request.operation {
            OpenDesktopOperation::Mastering {} => self.open_in_desktop(),
            OpenDesktopOperation::Studio {} => self.open_studio_in_desktop(),
            OpenDesktopOperation::Arrangement(value) => {
                self.open_arrangement_in_desktop(Parameters(value))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region_edit_test_support::Workspace;
    use serde_json::json;

    #[test]
    fn delivery_schemas_keep_object_roots_and_require_correct_target_ids() {
        for schema in [
            schemars::schema_for!(ExportResultRequest),
            schemars::schema_for!(OpenDesktopRequest),
        ] {
            let schema = serde_json::to_value(schema).unwrap();
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["additionalProperties"], false);
            assert_eq!(schema["required"], json!(["operation"]));
        }
        for invalid in [
            json!({"operation":{"action":"studio","preview_id":"master-id","name":"Mix"}}),
            json!({"operation":{"action":"mastering","studio_preview_id":"studio-id","name":"Mix"}}),
            json!({"operation":{"action":"raw_stems","stem_set_id":"bank","name":"Mix"}}),
        ] {
            assert!(serde_json::from_value::<ExportResultRequest>(invalid).is_err());
        }
        assert!(
            serde_json::from_value::<OpenDesktopRequest>(
                json!({"operation":{"action":"studio","session_id":"tracks"}})
            )
            .is_err()
        );
    }

    #[test]
    fn export_and_desktop_wrappers_preserve_existing_preflight_guards() {
        let workspace = Workspace::new();
        let companion = Companion::new(workspace.engine.clone());
        let before = workspace.engine.session().unwrap();
        for operation in [
            json!({"action":"mastering","preview_id":"stale-master","name":"Mix"}),
            json!({"action":"studio","studio_preview_id":"stale-studio","name":"Mix"}),
            json!({"action":"arrangement","session_id":"stale-arrangement","expected_revision":0,"name":"Mix"}),
            json!({"action":"raw_stems","stem_set_id":"stale-bank"}),
        ] {
            assert!(
                companion
                    .export_result(Parameters(
                        serde_json::from_value(json!({"operation":operation})).unwrap()
                    ))
                    .is_err()
            );
        }
        for operation in [
            json!({"action":"mastering"}),
            json!({"action":"studio"}),
            json!({"action":"arrangement","session_id":"stale-arrangement","expected_revision":0}),
        ] {
            assert!(
                companion
                    .open_desktop(Parameters(
                        serde_json::from_value(json!({"operation":operation})).unwrap()
                    ))
                    .is_err()
            );
        }
        assert_eq!(workspace.engine.session().unwrap(), before);
        let store = workspace.engine.lock().unwrap();
        assert!(store.jobs.is_empty() && store.active.is_none());
    }
}
