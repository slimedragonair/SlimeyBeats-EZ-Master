//! Explicit MCP preparation policy; ordinary audio tools never call these installers.
use crate::{jobs::Product, state::Engine};
use anyhow::{Result, ensure};
use mastercraft::{
    component_tools::{self, ToolKind},
    studio_native::plugins::VstHost,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicBool};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OptionalTool {
    AudioCodecs,
    Vst3Host,
}

/// Refresh discovery so preparing a missing helper does not require a reconnect.
pub(crate) fn plugin_host(engine: &Engine) -> Option<VstHost> {
    engine
        .plugin_host
        .clone()
        .filter(|host| host.executable.is_file())
        .or_else(VstHost::find)
}

pub fn status(engine: &Engine, selected: Option<OptionalTool>) -> Value {
    let tools = selected.map_or_else(
        || vec![OptionalTool::AudioCodecs, OptionalTool::Vst3Host],
        |tool| vec![tool],
    );
    json!({"tools":tools.into_iter().map(|tool| describe(engine, tool)).collect::<Vec<_>>(),
        "network":"Read-only local lookup; no download, catalog request or application restart.",
        "availability":"Codec availability is shared-runtime readiness. Host availability is detection of a candidate; inspect verification to distinguish a private executable hash check from an unverified bundled helper.",
        "versions":"Component and installed application versions are unknown here. The server library version/build is not an installed application or helper update receipt.",
        "next":"Only after the owner requests/agrees to setup, call prepare_optional_tool with tool and allow_download:true, then poll its job. Ordinary audio and plugin tools never download missing components."})
}

fn describe(engine: &Engine, tool: OptionalTool) -> Value {
    match tool {
        OptionalTool::AudioCodecs => {
            let available = mastercraft::optional_tools::available();
            observation(
                tool,
                available,
                if available {
                    "shared_runtime_ready"
                } else {
                    "not_prepared_or_missing"
                },
                None,
                mastercraft::optional_tools::supported(),
                "Existing system pair or the same private FFmpeg cache used by the GUI. Readiness comes from the shared runtime; a PATH filename alone is not readiness. System capability detection is deferred to explicit preparation.",
            )
        }
        OptionalTool::Vst3Host => {
            let private = component_tools::locate(ToolKind::PluginHost);
            let host = plugin_host(engine);
            let verified = private
                .as_ref()
                .zip(host.as_ref())
                .is_some_and(|(path, host)| *path == host.executable);
            observation(
                tool,
                host.is_some(),
                if verified {
                    "private_executable_hash_verified"
                } else if host.is_some() {
                    "detected_only"
                } else {
                    "missing"
                },
                verified.then_some(true),
                cfg!(all(windows, target_arch = "x86_64")),
                "A bundled helper is detected, not certified by its filename or the server build. A private helper is checked against its preparation receipt. Neither status establishes a component/application version. Plugin execution still requires owner-authorized --plugin-root folders.",
            )
        }
    }
}

fn observation(
    tool: OptionalTool,
    available: bool,
    verification: &str,
    hashes_verified: Option<bool>,
    download_supported: bool,
    detail: &str,
) -> Value {
    json!({"tool":tool,"available":available,"verification":verification,"hashes_verified":hashes_verified,
        "download_supported":download_supported,"component_version":null,"installed_application_version":null,
        "detail":detail})
}

pub(crate) fn start(
    engine: &Arc<Engine>,
    tool: OptionalTool,
    allow_download: bool,
) -> Result<Value> {
    start_with(engine, tool, allow_download, prepare)
}

fn start_with(
    engine: &Arc<Engine>,
    tool: OptionalTool,
    allow_download: bool,
    prepare: impl FnOnce(OptionalTool, Arc<AtomicBool>, &dyn Fn(f32, &str)) -> Result<Value>
    + Send
    + 'static,
) -> Result<Value> {
    // Reject before reserving a job, touching a cache, or entering shared setup code.
    ensure!(
        allow_download,
        "Optional tool setup requires allow_download:true after the owner requests/agrees to the download. get_optional_tool_status is read-only."
    );
    engine.start(
        "prepare_optional_tool",
        |_| Ok(()),
        move |ctx| {
            ctx.check()?;
            let result = prepare(tool, ctx.cancel.clone(), &|fraction, message| {
                ctx.progress(fraction, message)
            })?;
            ctx.check()?;
            // A finished cache may survive cancellation; never publish cancelled work as success.
            Ok(Product::value(result))
        },
    )
}

fn prepare(
    tool: OptionalTool,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(f32, &str),
) -> Result<Value> {
    let mut result = match tool {
        OptionalTool::AudioCodecs => {
            let runtime = mastercraft::optional_tools::ensure(progress, cancel)?;
            json!({"verification":"encode_probe_decode_checked","hashes_verified":runtime.private,
                "source":if runtime.private {"verified_private_cache"} else {"system_tools_not_hash_pinned"}})
        }
        OptionalTool::Vst3Host => {
            component_tools::ensure(ToolKind::PluginHost, cancel, progress)?;
            json!({"verification":"private_executable_hash_verified","hashes_verified":true,
                "source":"official_catalog_private_cache","preparation_scope":"private_recovery_cache","bundled_host_validated":false,"bundled_host_replaced":false})
        }
    };
    result["tool"] = serde_json::to_value(tool)?;
    result["available"] = json!(true);
    result["component_version"] = Value::Null;
    result["installed_application_updated"] = json!(false);
    result["next"] = json!(if tool == OptionalTool::Vst3Host {
        "Private recovery cache prepared. Recheck get_optional_tool_status before retrying; normal discovery may still select a bundled candidate, which this action did not validate or replace. No audio, app restart, AI-client edit or plugin download was performed."
    } else {
        "Retry the requested operation explicitly. No audio was changed; no application restart, AI-client edit, or plugin download was performed."
    });
    Ok(result)
}

/// Discovery metadata for the enforced shared-runtime bounds, not process RAM limits.
pub fn policy() -> Value {
    json!({"tools":["audio_codecs","vst3_host"],"status_tool":"get_optional_tool_status","prepare_tool":"prepare_optional_tool",
        "allow_download_default":false,"consent":"Only set allow_download:true when the owner requests/agrees to optional tool preparation. No unrelated tool automatically downloads components.",
        "audio_codecs":{"when":"Compressed audio input/export or changing sample rate. Native WAV analysis, mastering, same-rate WAV export, voice cleanup and spectral repair need no codec download.",
            "max_download_bytes":{"windows_x86_64":192_u64*1024*1024,"linux_x86_64_gnu":160_u64*1024*1024},"max_expanded_bytes":640_u64*1024*1024,
            "source":"Existing capable FFmpeg/ffprobe pair first, otherwise the compiled SHA-256-pinned bundle in the same private user cache as the GUI. Linux private setup also requires system xz.","linux_xz_memory_limit_bytes":256_u64*1024*1024},
        "vst3_host":{"when":"The owner explicitly requests private host recovery/preparation. This can download even when a bundled candidate is detected; it does not validate or replace that candidate, and normal discovery may continue to select it.","preparation_mode":"explicit_private_recovery","reuses_bundled_host":false,"download_platform":"windows_x86_64","max_download_bytes":192_u64*1024*1024,"max_expanded_bytes":384_u64*1024*1024,"max_entry_bytes":128_u64*1024*1024,
            "source":"Only application-owned helper assets from the fixed official GitHub release/catalog, with archive/file hashes and protocol checks. No arbitrary URL, plugin, installer launch or client configuration is accepted. Availability depends on the official catalog supplying a compatible recovery bundle; otherwise use the full installer manually or build the helper from source."},
        "jobs":"One heavy job per connection. Poll every 250-1000 ms; cancel_job requests cancellation. Wait for a terminal status before retrying. 100% means completed; a successfully prepared cache may remain after cancellation.",
        "privacy":"Setup downloads software, never audio. Audio stays local; the AI client controls its metadata privacy. Download/expanded-byte caps are not total RAM or disk requirements; staging, decompression and audio working buffers require additional resources.",
        "application_updates":"These tools do not update the installed application and do not infer its version from this server's library Build 2. No unasked app restart or AI-client configuration edit.",
        "stem_engine":"Separate optional model workflow: get_stem_engine_status/install_stem_engine only after owner-authorized setup. separate_stems never installs it automatically."})
}

pub fn help() -> Value {
    let mut value = policy();
    value["steps"] = json!([
        {"tool":"get_optional_tool_status","arguments":{},"read":"Available reports shared codec readiness or a detected host candidate; inspect verification. Neither establishes an application version. Bundled host detection is not hash verification."},
        {"tool":"prepare_optional_tool","arguments":{"tool":"audio_codecs","allow_download":true},"when":"Only after the owner requests/agrees to setup for compressed inputs/exports or resampling. Existing capable tools may be reused without a download."},
        {"tool":"prepare_optional_tool","arguments":{"tool":"vst3_host","allow_download":true},"when":"Only after owner agreement to explicit private host recovery/preparation. It may download despite a bundled candidate, which it does not validate or replace. It never downloads third-party plugins or grants plugin-root access."},
        {"tool":"get_job_status","arguments":{"job_id":"<returned job_id>"},"read":"Poll until completed/failed/cancelled, then explicitly retry the original operation. Completed result reports the preparation evidence."},
        {"tool":"cancel_job","arguments":{"job_id":"<active job_id>"},"when":"Owner wants setup stopped. Wait for a terminal status. A completed private cache can remain, but cancellation never resumes audio/plugin work."}
    ]);
    value
}

#[cfg(test)]
#[path = "optional_tool_tests.rs"]
mod tests;
