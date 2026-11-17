//! Compact aligned-bank imports delegate to the existing guarded handlers.
use crate::{
    protocol::{Companion, LibraryPath},
    stem_bundle_protocol::ImportBundle,
    stem_protocol::ImportRequest,
};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct StemImportRequest {
    pub operation: StemImportOperation,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum StemImportOperation {
    /// Review ZIP parts and alternate WAV/MP3 encodings without replacing Studio.
    InspectZip(LibraryPath),
    /// Import the reviewed ZIP; omitted parts selects all distinct parts.
    Zip(ImportBundle),
    /// Import explicitly identified aligned files from authorized libraries.
    Files(ImportRequest),
}

#[tool_router(router=compact_stem_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "operation.action: inspect_zip | zip | files. inspect_zip reviews a Suno/stem ZIP synchronously using zero-based library and its relative path; inspect before importing. Matching WAV/MP3 versions are one part, WAV preferred by default. zip uses the same library/path plus optional preferred_format and parts[{part_index,kind}]; part_index is zero-based, omit parts for all distinct parts. files uses files[{kind,library,path}] with unique stable kinds. zip/files require at least two aligned parts with matching sample rate, channels and frames; no hidden trimming/resampling or fixed count cap, resource budgets still apply. For independent tracks or different durations use Tracks arrangement tools; get_help(topic=arrangement). Import replaces only Studio with an immutable bank and temporary preview. Poll get_job_status for job_id until terminal, then use the returned stem_set_id/studio_preview_id. Saving/export is separate and requires the user's request.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn stem_import(
        &self,
        Parameters(request): Parameters<StemImportRequest>,
    ) -> Result<Json<Value>, String> {
        match request.operation {
            StemImportOperation::InspectZip(arguments) => {
                self.inspect_stem_bundle(Parameters(arguments))
            }
            StemImportOperation::Zip(arguments) => self.import_stem_bundle(Parameters(arguments)),
            StemImportOperation::Files(arguments) => self.import_stems(Parameters(arguments)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region_edit_test_support::Workspace;
    use serde_json::json;

    #[test]
    fn stem_import_requires_an_object_with_a_strict_typed_action() {
        let tools = Companion::compact_stem_router().list_all();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "stem_import");
        assert_eq!(tools[0].input_schema["type"], "object");
        let schema = serde_json::to_value(schemars::schema_for!(StemImportRequest)).unwrap();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        for arguments in [
            json!({"operation":{"action":"inspect_zip","library":0,"path":"stems.zip"}}),
            json!({"operation":{"action":"zip","library":0,"path":"stems.zip","parts":[{"part_index":0},{"part_index":1}]}}),
            json!({"operation":{"action":"files","files":[{"kind":"vocals","library":0,"path":"voice.wav"},{"kind":"part_64","library":0,"path":"backing.wav"}]}}),
        ] {
            serde_json::from_value::<StemImportRequest>(arguments).unwrap();
        }
        for arguments in [
            json!({"action":"inspect_zip","library":0,"path":"stems.zip"}),
            json!({"operation":{"action":"inspect_zip","library":0}}),
            json!({"operation":{"action":"zip","library":0,"path":"stems.zip","preferred_format":"flac"}}),
            json!({"operation":{"action":"files","files":[],"export":true}}),
            json!({"operation":{"action":"export_stems","stem_set_id":"stems-current"}}),
            json!({"operation":{"action":"inspect_zip","library":0,"path":"stems.zip"},"tool":"load_audio"}),
        ] {
            assert!(serde_json::from_value::<StemImportRequest>(arguments).is_err());
        }
    }

    #[test]
    fn compact_import_retains_path_and_minimum_aligned_bank_guards() {
        let workspace = Workspace::new();
        let companion = Companion::new(workspace.engine.clone());
        let before = workspace.engine.session().unwrap();
        for action in ["inspect_zip", "zip"] {
            let arguments =
                json!({"operation":{"action":action,"library":0,"path":"../outside.zip"}});
            let result =
                companion.stem_import(Parameters(serde_json::from_value(arguments).unwrap()));
            assert!(result.is_err());
        }
        let arguments = json!({"operation":{"action":"files","files":[{"kind":"vocals","library":0,"path":"voice.wav"}]}});
        let error = companion
            .stem_import(Parameters(serde_json::from_value(arguments).unwrap()))
            .err()
            .expect("A single aligned part must be refused");
        assert!(error.contains("at least two aligned stems"), "{error}");
        assert_eq!(workspace.engine.session().unwrap(), before);
        let store = workspace.engine.lock().unwrap();
        assert!(store.jobs.is_empty() && store.active.is_none());
    }
}
