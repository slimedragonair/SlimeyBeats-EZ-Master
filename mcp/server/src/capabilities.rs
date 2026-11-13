use crate::state::{Engine, public_settings};
use serde_json::{Value, json};

pub fn describe(engine: &Engine) -> Value {
    let profiles = mastercraft::profiles::builtin()
        .into_iter()
        .map(|(id, p)| json!({"id":id,"name":p["display_name"],"targets_lufs":p["lufs"]}))
        .collect::<Vec<_>>();
    json!({"companion_version":env!("CARGO_PKG_VERSION"),"engine_version":mastercraft::branding::ENGINE_VERSION,
        "profiles":profiles,"export_formats":["wav16","wav24","wav32f","flac16","flac24","mp3"],
        "defaults":public_settings(&mastercraft::config::Settings::default()),
        "module_ids":mastercraft::config::MODULES,
        "loudness_targets":"profiles[].targets_lufs lists each mode's requested LUFS target. An explicit target or reference loudness can replace it; gain/limiter limits can prevent reaching it. Tonal intensity does not scale the loudness target.",
        "libraries":engine.policy.inputs.iter().enumerate().map(|(index,path)|json!({"index":index,"name":path.file_name().unwrap_or_default().to_string_lossy()})).collect::<Vec<_>>(),
        "max_decoded_input_mib":engine.max_decoded_bytes / (1024*1024),"decode_timeout_seconds":engine.decode_timeout.as_secs(),
        "concurrent_audio_jobs":1,"desktop_configured":engine.desktop.is_some(),
        "privacy":"Local stdio only. This companion does not upload audio. Your AI client may send tool requests, measurements and filenames to its model provider. No audio is sent through tool results.",
        "memory":"The input limit is decoded PCM, not total process memory. Mastering holds several working buffers. One source and one master per session; longer tracks depend on RAM.",
        "workflow":["get_help for complete examples and units","list_audio_files","load_audio and poll for source_id","analyze_audio","master_preview and poll for preview_id","optional suggest_eq then apply_eq","open_in_desktop to audition","export_audio only when requested"],
        "help_topics":["quick_start","mastering","eq","export","troubleshooting","privacy"]})
}
