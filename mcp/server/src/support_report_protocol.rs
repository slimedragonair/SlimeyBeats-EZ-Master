//! Small two-step local support workflow; no posting or recovery-cache access.
use crate::{protocol::Companion, support_reports};
use mastercraft::support_report::IssueCategory;
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareSupportReport {
    /// Generic issue category; default other. Never include a song/plugin/profile name here.
    #[serde(default)]
    pub category: IssueCategory,
    /// Deliberately user-entered title, at most 512 UTF-8 bytes. Default empty. Do not autofill with raw errors, names, paths or logs. Returned text reaches this AI client/model provider; obtain owner agreement before sending sensitive text.
    #[serde(default)]
    #[schemars(length(max = 512))]
    pub title: String,
    /// Deliberately user-entered description/reproduction steps, at most 16384 UTF-8 bytes. Default empty. Obvious paths/emails/secrets are filtered best-effort; personal content may remain and requires full owner review. The AI client/provider may already receive this input before filtering. No automatic error/log collection.
    #[serde(default)]
    #[schemars(length(max = 16384))]
    pub details: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveSupportReport {
    /// Exact review_id from this connection's latest successful prepare_support_report. A new successful preview invalidates the old ID. Not evidence of human consent.
    #[schemars(length(max = 128))]
    pub review_id: String,
    /// Fresh .md filename only, at most 180 UTF-8 bytes; no directory, traversal, absolute path, reserved device name or overwrite. Saved directly under the owner-authorized output root.
    #[schemars(length(max = 180))]
    pub filename: String,
    /// Default false. Set true ONLY after the owner has reviewed the entire matching preview and explicitly approved saving those exact bytes. This records the caller's assertion; the backend cannot verify that a human actually reviewed it. No upload/issue posting occurs.
    #[serde(default)]
    pub owner_review_confirmed: bool,
}

#[tool_router(router=support_report_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Prepare a support-report preview using allowlisted connection counts/booleans and optional non-sensitive user text. This companion does not read files/audio/projects/logs/errors/environment, probe tool readiness, upload files or post issues. IMPORTANT: tool arguments and returned report text may reach your AI client's model provider before review or saving. Filtering cannot retract text already sent to that client; use only non-sensitive descriptions. Filtering is best-effort and typed text may remain sensitive: show the ENTIRE preview to the owner. Keeps one immutable report in memory per connection; successful preparation replaces its review_id, failures preserve the old report and audio session. Companion version and linked_engine_build are separate; installed desktop version stays unknown. Returns preview_markdown, review_id, bytes and notices; get_help(support_reports) explains the next step.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    fn prepare_support_report(
        &self,
        Parameters(a): Parameters<PrepareSupportReport>,
    ) -> Result<Json<Value>, String> {
        self.response(support_reports::prepare(
            &self.engine,
            &self.support_reports,
            a,
        ))
    }

    #[tool(
        description = "Save EXACT bytes of the latest immutable reviewed support-report preview into a fresh .md filename under the owner-authorized output root. Requires matching review_id and owner_review_confirmed:true ONLY after the owner reviewed the complete preview and explicitly requested saving it. Caller assertion is not proof of human consent. Refuses stale/foreign IDs, paths and existing files; never overwrites, uploads, posts issues or launches a browser. Saves are local, but MCP results and previously returned report text reach the AI client/model provider. Failed saves preserve the usable preview and all audio/project/settings state. No access to the desktop's private recovery cache.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn save_support_report(
        &self,
        Parameters(a): Parameters<SaveSupportReport>,
    ) -> Result<Json<Value>, String> {
        self.response(support_reports::save(
            &self.engine,
            &self.support_reports,
            a,
        ))
    }
}
