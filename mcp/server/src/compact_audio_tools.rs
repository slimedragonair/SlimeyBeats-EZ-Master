//! Typed audio operations reuse the established jobs and source guards.
use crate::{
    protocol::{Companion, LibraryPath, SourceId},
    studio_protocol::StudioAudioRequest,
};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReferenceRequest {
    pub operation: ReferenceOperation,
}
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReferenceOperation {
    Load(LibraryPath),
    Clear {},
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalyzeRequest {
    pub operation: AnalyzeOperation,
}
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnalyzeOperation {
    Mastering(SourceId),
    Studio(StudioAudioRequest),
}

#[tool_router(router=compact_audio_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Manage the mastering reference. operation.action=load requires authorized library/path; clear takes no other fields. Either returns a job: poll get_job_status. Reference changes invalidate the mastering preview, so master again before export. Does not alter sources or arrangement EQ.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn reference(
        &self,
        Parameters(a): Parameters<ReferenceRequest>,
    ) -> Result<Json<Value>, String> {
        match a.operation {
            ReferenceOperation::Load(request) => self.load_reference(Parameters(request)),
            ReferenceOperation::Clear {} => self.clear_reference(),
        }
    }

    #[tool(
        description = "Measure audio without applying changes. operation.action=mastering requires current source_id; studio requires audio_id from get_session.studio.assets. Returns a job: poll get_job_status for loudness, peaks and tonal evidence. This is measurement, not a listening-quality verdict; use scan_audio_issues for timestamped issue regions.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn analyze(
        &self,
        Parameters(a): Parameters<AnalyzeRequest>,
    ) -> Result<Json<Value>, String> {
        match a.operation {
            AnalyzeOperation::Mastering(request) => self.analyze_audio(Parameters(request)),
            AnalyzeOperation::Studio(request) => self.analyze_studio_audio(Parameters(request)),
        }
    }
}
