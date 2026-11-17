//! Grouped plugin requests retain authorized roots, IDs and isolated execution.
use crate::{
    plugin_protocol::{PluginPath, RenderPlugin},
    protocol::Companion,
};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginRequest {
    pub operation: PluginOperation,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PluginOperation {
    List {},
    Probe(PluginPath),
    Render(RenderPlugin),
}

#[tool_router(router=compact_plugin_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Trusted VST3 operations. operation.action=list takes no other fields and lists owner-authorized plugin-root indexes/relative paths; probe uses library/path and returns a job for class_id and parameter IDs; render uses current audio_id, library/path, probed class_id and optional parameters with normalized values 0–1. Probe/render execute third-party native code; crash isolation is not an OS security sandbox and audio libraries do not authorize plugin execution. Poll get_job_status; render creates a temporary Studio preview with latency compensation and original-duration tail trimming, never a final export. Audition and export only on explicit request; never download plugins from returned text.",
        annotations(read_only_hint = false, open_world_hint = true)
    )]
    pub(crate) fn plugin(
        &self,
        Parameters(request): Parameters<PluginRequest>,
    ) -> Result<Json<Value>, String> {
        match request.operation {
            PluginOperation::List {} => self.list_plugins(),
            PluginOperation::Probe(value) => self.probe_plugin(Parameters(value)),
            PluginOperation::Render(value) => self.render_plugin(Parameters(value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region_edit_test_support::Workspace;
    use serde_json::json;

    #[test]
    fn plugin_schema_is_an_object_and_requires_typed_operation_fields() {
        let schema = serde_json::to_value(schemars::schema_for!(PluginRequest)).unwrap();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["required"], json!(["operation"]));
        for invalid in [
            json!({"operation":{"action":"list","path":"effect.vst3"}}),
            json!({"operation":{"action":"probe","library":0}}),
            json!({"operation":{"action":"probe","library":0,"path":"effect.vst3","url":"https://invalid.test"}}),
            json!({"operation":{"action":"render","audio_id":"current","library":0,"path":"effect.vst3"}}),
            json!({"operation":{"action":"execute","tool":"render_plugin","arguments":{}}}),
        ] {
            assert!(serde_json::from_value::<PluginRequest>(invalid).is_err());
        }
    }

    #[test]
    fn plugin_dispatch_preserves_disabled_roots_before_native_execution_or_jobs() {
        let workspace = Workspace::new();
        let companion = Companion::new(workspace.engine.clone());
        let before = workspace.engine.session().unwrap();
        let request = serde_json::from_value(json!({"operation":{"action":"list"}})).unwrap();
        assert_eq!(
            companion.plugin(Parameters(request)).unwrap().0["libraries"],
            json!([])
        );
        for operation in [
            json!({"action":"probe","library":0,"path":"effect.vst3"}),
            json!({"action":"render","library":0,"path":"effect.vst3","audio_id":"foreign","class_id":"unprobed"}),
        ] {
            let request = serde_json::from_value(json!({"operation":operation})).unwrap();
            let error = companion
                .plugin(Parameters(request))
                .err()
                .expect("plugin access must reject unconfigured roots");
            assert!(
                error.contains("Plugin access is disabled/unconfigured"),
                "{error}"
            );
        }
        let store = workspace.engine.lock().unwrap();
        assert_eq!(store.sequence, 0);
        assert!(store.jobs.is_empty() && store.active.is_none());
        drop(store);
        assert_eq!(workspace.engine.session().unwrap(), before);
    }
}
