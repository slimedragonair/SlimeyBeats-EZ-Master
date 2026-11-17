//! Self-describing optional-tool routes, separate from audio editing and model setup.
use crate::{
    optional_tools::{self, OptionalTool},
    protocol::Companion,
};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolStatusRequest {
    /// Omit to inspect both audio_codecs and vst3_host. Read-only lookup never downloads.
    pub tool: Option<OptionalTool>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareToolRequest {
    /// audio_codecs for FFmpeg/ffprobe; vst3_host explicitly prepares the private official host recovery cache, even when a bundled candidate exists. Never downloads third-party plugins.
    pub tool: OptionalTool,
    /// Defaults false. Set true only after owner agreement. Codecs may reuse capable system tools. VST preparation reuses only a verified private cache; it does not validate/replace a bundled host and may download despite one being detected. No audio upload, application restart or AI-client configuration edit.
    #[serde(default)]
    pub allow_download: bool,
}

#[tool_router(router=optional_tool_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Inspect audio_codecs / vst3_host availability without network or installation. Omit tool to inspect both. Distinguishes shared codec readiness, a hash-verified private host and a merely detected bundled host. Unknown component/application versions remain unknown; server library Build 2 is not an installed-app update receipt.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) fn get_optional_tool_status(
        &self,
        Parameters(a): Parameters<ToolStatusRequest>,
    ) -> Json<Value> {
        Json(optional_tools::status(&self.engine, a.tool))
    }

    #[tool(
        description = "Explicitly prepare audio_codecs or the official private vst3_host recovery cache shared with the GUI. Requires allow_download:true after owner agreement; default false rejects before setup. Codecs may reuse capable system tools. VST preparation reuses only a verified private cache, does not validate/replace the selected bundled host, and may download even when a bundled candidate exists. Codec archive caps: Windows 192 MiB / Linux 160 MiB; host 192 MiB Windows x64. No arbitrary URL/plugin download, audio upload, app restart or AI-client edit. Returns a cancellable job; poll before explicitly retrying. Stem-model setup stays separate/manual. Read get_help(optional_tools) for resource limits.",
        annotations(destructive_hint = false, open_world_hint = true)
    )]
    pub(crate) fn prepare_optional_tool(
        &self,
        Parameters(a): Parameters<PrepareToolRequest>,
    ) -> Result<Json<Value>, String> {
        self.response(optional_tools::start(
            &self.engine,
            a.tool,
            a.allow_download,
        ))
    }
}
