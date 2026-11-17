//! Compact discovery and explicit setup keep the established installer boundaries.
use crate::{
    optional_tool_protocol::{PrepareToolRequest, ToolStatusRequest},
    protocol::Companion,
    studio_protocol::EngineSetupRequest,
};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetupOptionalToolRequest {
    pub operation: SetupOptionalToolOperation,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SetupOptionalToolOperation {
    Component(PrepareToolRequest),
    StemEngine {
        /// Defaults false. Repair the private runtime/model only after owner-authorized setup.
        #[serde(default)]
        repair: bool,
        /// Defaults false. Requires explicit owner agreement before installation or repair.
        #[serde(default)]
        allow_download: bool,
    },
}

#[tool_router(router=compact_optional_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Inspect optional local tools without downloading or installing. Omit tool to check audio_codecs and vst3_host, or set tool to one of those IDs; stem-engine status is always included. Component detection does not certify an installed application version. For setup use setup_optional_tool with an explicit operation.action and owner-agreed allow_download:true; poll its job before retrying the original operation.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) fn optional_tool_status(
        &self,
        Parameters(request): Parameters<ToolStatusRequest>,
    ) -> Result<Json<Value>, String> {
        let components = self.get_optional_tool_status(Parameters(request));
        let stem_engine = self.get_stem_engine_status();
        Ok(Json(
            json!({"components": components.0, "stem_engine": stem_engine.0}),
        ))
    }

    #[tool(
        description = "Explicit owner-authorized optional setup. operation.action=component uses tool=audio_codecs or vst3_host and allow_download:true; stem_engine uses allow_download:true and optional repair (default false). Missing/false consent rejects before installation or job reservation. Stem setup downloads an isolated CPU runtime/model; component setup preserves the existing verified-cache and authorized-root policies. No arbitrary URLs, third-party plugin installation, audio upload, app restart or client-config edit. Returns job_id; poll get_job_status until terminal, then explicitly retry the original operation.",
        annotations(destructive_hint = false, open_world_hint = true)
    )]
    pub(crate) fn setup_optional_tool(
        &self,
        Parameters(request): Parameters<SetupOptionalToolRequest>,
    ) -> Result<Json<Value>, String> {
        match request.operation {
            SetupOptionalToolOperation::Component(value) => {
                self.prepare_optional_tool(Parameters(value))
            }
            SetupOptionalToolOperation::StemEngine {
                repair,
                allow_download,
            } => {
                if !allow_download {
                    return Err("Stem-engine setup requires allow_download:true after the owner requests/agrees to installation or repair.".into());
                }
                self.install_stem_engine(Parameters(EngineSetupRequest { repair }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{jobs::ActiveJob, region_edit_test_support::Workspace};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn setup_schema_is_an_object_and_rejects_untyped_dispatch_or_extra_fields() {
        let schema = serde_json::to_value(schemars::schema_for!(SetupOptionalToolRequest)).unwrap();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["required"], json!(["operation"]));
        for invalid in [
            json!({"action":"stem_engine","allow_download":true}),
            json!({"operation":{"action":"install_stem_engine"}}),
            json!({"operation":{"action":"stem_engine","url":"https://invalid.test"}}),
            json!({"operation":{"action":"component","tool":"arbitrary_plugin"}}),
            json!({"operation":{"action":"component","tool":"vst3_host","allow_download":"true"}}),
        ] {
            assert!(serde_json::from_value::<SetupOptionalToolRequest>(invalid).is_err());
        }
    }

    #[test]
    fn setup_consent_precedes_job_admission_and_preserves_existing_work() {
        let workspace = Workspace::new();
        let companion = Companion::new(workspace.engine.clone());
        // Even a broken consent dispatch cannot start a real installer in this fixture.
        workspace.engine.lock().unwrap().active = Some(ActiveJob {
            id: "existing-job".into(),
            cancel: Arc::new(AtomicBool::new(false)),
        });
        let before = workspace.engine.session().unwrap();
        for operation in [
            json!({"action":"stem_engine"}),
            json!({"action":"stem_engine","repair":true,"allow_download":false}),
            json!({"action":"component","tool":"audio_codecs"}),
            json!({"action":"component","tool":"vst3_host","allow_download":false}),
        ] {
            let request = serde_json::from_value(json!({"operation":operation})).unwrap();
            let error = companion
                .setup_optional_tool(Parameters(request))
                .err()
                .expect("setup must reject missing consent");
            assert!(error.contains("allow_download:true"), "{error}");
        }
        let store = workspace.engine.lock().unwrap();
        assert_eq!(store.sequence, 0);
        assert!(store.jobs.is_empty());
        assert_eq!(store.active.as_ref().unwrap().id, "existing-job");
        assert!(
            !store
                .active
                .as_ref()
                .unwrap()
                .cancel
                .load(Ordering::Relaxed)
        );
        drop(store);
        assert_eq!(workspace.engine.session().unwrap(), before);
    }

    #[test]
    fn filtered_component_status_still_includes_the_stem_engine_without_jobs() {
        let workspace = Workspace::new();
        let companion = Companion::new(workspace.engine.clone());
        for (request, count) in [(json!({}), 2), (json!({"tool":"vst3_host"}), 1)] {
            let result = companion
                .optional_tool_status(Parameters(serde_json::from_value(request).unwrap()))
                .unwrap()
                .0;
            assert_eq!(
                result["components"]["tools"].as_array().unwrap().len(),
                count
            );
            assert!(result["stem_engine"].is_object());
        }
        let store = workspace.engine.lock().unwrap();
        assert_eq!(store.sequence, 0);
        assert!(store.jobs.is_empty() && store.active.is_none());
    }
}
