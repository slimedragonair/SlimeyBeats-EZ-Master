//! MCP is a thin adapter: protocol schemas live here, DSP and job state elsewhere.
use crate::{
    help::{self, HelpRequest},
    jobs::Product,
    options::{EqOptions, ExportFormat, MasterOptions},
    processing,
    state::Engine,
};
use rmcp::{
    Json, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone)]
pub struct Companion {
    pub(crate) engine: Arc<Engine>,
    tool_router: ToolRouter<Self>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibraryPath {
    /// Zero-based index of a folder authorized when starting the companion.
    pub library: usize,
    /// Path relative to this library. Files outside the library are rejected.
    pub path: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListFiles {
    pub library: usize,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "page_size")]
    pub limit: usize,
}
fn page_size() -> usize {
    50
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceId {
    pub source_id: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobId {
    pub job_id: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MasterRequest {
    pub source_id: String,
    #[serde(default)]
    pub options: MasterOptions,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EqRequest {
    pub preview_id: String,
    pub options: EqOptions,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportRequest {
    pub preview_id: String,
    /// A filename without an extension or directory. Existing files are preserved.
    pub name: String,
    #[serde(default)]
    pub format: ExportFormat,
    /// Zero keeps the source rate. Resampling requires FFmpeg.
    #[serde(default)]
    pub sample_rate: u32,
    #[serde(default = "enabled")]
    pub dither: bool,
}
fn enabled() -> bool {
    true
}

impl Companion {
    pub(crate) fn response(&self, value: anyhow::Result<Value>) -> Result<Json<Value>, String> {
        value
            .map(Json)
            .map_err(|e| self.engine.policy.redact(&format!("{e:#}")))
    }
}

#[tool_router]
impl Companion {
    pub fn new(engine: Arc<Engine>) -> Self {
        Self {
            engine,
            tool_router: Self::tool_router()
                + Self::studio_router()
                + Self::plugin_router()
                + Self::stem_router(),
        }
    }

    #[tool(
        description = "Discover bundled profiles, export formats, folder indexes and local resource limits. Call before processing.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn get_capabilities(&self) -> Json<Value> {
        Json(crate::capabilities::describe(&self.engine))
    }

    #[tool(
        description = "Self-contained guide for an assistant with no project history. Defaults to quick_start with complete tool-call examples, IDs and job polling. Other topics explain mastering/EQ units, export, privacy and error recovery.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn get_help(&self, Parameters(a): Parameters<HelpRequest>) -> Json<Value> {
        Json(help::get(a.topic))
    }

    #[tool(
        description = "List a bounded page of audio files in an authorized library folder. Does not recurse.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn list_audio_files(
        &self,
        Parameters(a): Parameters<ListFiles>,
    ) -> Result<Json<Value>, String> {
        self.response(
            self.engine
                .policy
                .list(a.library, &a.folder, a.offset, a.limit),
        )
    }

    #[tool(
        description = "Start importing one local source. Poll its job. Successful import replaces the previous session; failed imports preserve it.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn load_audio(&self, Parameters(a): Parameters<LibraryPath>) -> Result<Json<Value>, String> {
        let result = (|| {
            let path = self.engine.policy.audio(a.library, &a.path)?;
            self.engine.start(
                "load_audio",
                |_| Ok(()),
                move |ctx| processing::load(ctx, path, false),
            )
        })();
        self.response(result)
    }

    #[tool(
        description = "Import and analyze a reference track. Requires a source. Invalidates the old preview; no source audio is changed.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn load_reference(
        &self,
        Parameters(a): Parameters<LibraryPath>,
    ) -> Result<Json<Value>, String> {
        let result = (|| {
            let path = self.engine.policy.audio(a.library, &a.path)?;
            self.engine.start(
                "load_reference",
                |s| {
                    anyhow::ensure!(s.source.is_some(), "Load source audio first");
                    Ok(())
                },
                move |ctx| processing::load(ctx, path, true),
            )
        })();
        self.response(result)
    }

    #[tool(
        description = "Clear the reference and invalidate the preview. Re-master after changing the reference.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn clear_reference(&self) -> Result<Json<Value>, String> {
        self.response(self.engine.start(
            "clear_reference",
            |_| Ok(()),
            |_| {
                Ok(Product {
                    result: json!({"reference_loaded":false}),
                    change: crate::jobs::Change::Reference(None),
                    committed: false,
                })
            },
        ))
    }

    #[tool(
        description = "Measure original audio, including loudness, true peak and tonal issues. Returns a background job ID, not a quality verdict.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn analyze_audio(&self, Parameters(a): Parameters<SourceId>) -> Result<Json<Value>, String> {
        self.response(
            self.engine
                .start("analyze_audio", |s| s.source(&a.source_id).map(|_| ()), {
                    let id = a.source_id.clone();
                    move |ctx| processing::analyze(ctx, id, false)
                }),
        )
    }

    #[tool(
        description = "Suggest conservative deterministic EQ bands using the current profile. Uses the pre-EQ master when available. Does not apply them.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn suggest_eq(&self, Parameters(a): Parameters<SourceId>) -> Result<Json<Value>, String> {
        self.response(
            self.engine
                .start("suggest_eq", |s| s.source(&a.source_id).map(|_| ()), {
                    let id = a.source_id.clone();
                    move |ctx| processing::analyze(ctx, id, true)
                }),
        )
    }

    #[tool(
        description = "Master with the desktop Rust engine and prepare a temporary WAV preview. Retains current EQ. Does not export final audio. Poll the job, audition and adjust first.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn master_preview(
        &self,
        Parameters(a): Parameters<MasterRequest>,
    ) -> Result<Json<Value>, String> {
        self.response(self.engine.start(
            "master_preview",
            |s| {
                s.source(&a.source_id)?;
                a.options.apply(&s.settings, s.reference.is_some())?;
                Ok(())
            },
            {
                let a = a.clone_request();
                move |ctx| processing::master(ctx, a.source_id, a.options)
            },
        ))
    }

    #[tool(
        description = "Render seven-band EQ from the pre-EQ master, avoiding cumulative processing. Returns a new preview ID. Desktop controls provide real-time auditioning; this tool renders a background revision.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn apply_eq(&self, Parameters(a): Parameters<EqRequest>) -> Result<Json<Value>, String> {
        self.response(self.engine.start(
            "apply_eq",
            |s| {
                let m = s.master(&a.preview_id)?;
                a.options.apply(&m.settings)?;
                Ok(())
            },
            {
                let id = a.preview_id.clone();
                let options = a.options.clone();
                move |ctx| processing::equalize(ctx, id, options)
            },
        ))
    }

    #[tool(
        description = "Inspect the current source, preview IDs, settings and active job. Temporary preview paths are relative to the configured output folder.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn get_session(&self) -> Result<Json<Value>, String> {
        self.response(self.engine.session())
    }

    #[tool(
        description = "Poll a background job. Wait between polls. Completed jobs contain structured results; failed jobs contain an error. History retains 64 jobs.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn get_job_status(&self, Parameters(a): Parameters<JobId>) -> Result<Json<Value>, String> {
        self.response(self.engine.job(&a.job_id))
    }

    #[tool(
        description = "Request cooperative cancellation. Wait for a terminal status before starting another job. A final file already saved remains reported as completed.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn cancel_job(&self, Parameters(a): Parameters<JobId>) -> Result<Json<Value>, String> {
        self.response(self.engine.cancel(&a.job_id))
    }

    #[tool(
        description = "Explicitly export the current preview as a new audio file and JSON report. Only use when the user asks to save/export. Existing files are never replaced. FLAC/MP3 and resampling require FFmpeg.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn export_audio(
        &self,
        Parameters(a): Parameters<ExportRequest>,
    ) -> Result<Json<Value>, String> {
        let result = (|| {
            crate::file_policy::export_stem(&a.name)?;
            self.engine.start(
                "export_audio",
                |s| {
                    let m = s.master(&a.preview_id)?;
                    let mut settings = m.settings.clone();
                    settings.output_sr = a.sample_rate;
                    settings.validate()
                },
                {
                    let id = a.preview_id.clone();
                    move |ctx| {
                        processing::export_audio(ctx, id, a.name, a.format, a.sample_rate, a.dither)
                    }
                },
            )
        })();
        self.response(result)
    }

    #[tool(
        description = "Open a new desktop session to audition and adjust the result. Verifies source identities, saves a handoff document and launches the trusted configured executable. No final audio export.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn open_in_desktop(&self) -> Result<Json<Value>, String> {
        self.response(self.engine.start(
            "open_in_desktop",
            |s| {
                anyhow::ensure!(s.source.is_some(), "Load audio first");
                anyhow::ensure!(
                    self.engine.desktop.is_some(),
                    "Configure --desktop-exe first"
                );
                Ok(())
            },
            processing::desktop,
        ))
    }

    #[tool(
        description = "Release the source/master and delete their temporary previews. Requests cancellation of any running job. Final exports and source files are preserved.",
        annotations(destructive_hint = true, open_world_hint = false)
    )]
    fn clear_session(&self) -> Result<Json<Value>, String> {
        self.response(self.engine.clear())
    }
}

impl MasterRequest {
    fn clone_request(&self) -> Self {
        Self {
            source_id: self.source_id.clone(),
            options: self.options.clone(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Companion {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_instructions(help::INSTRUCTIONS.to_string())
    }
}
