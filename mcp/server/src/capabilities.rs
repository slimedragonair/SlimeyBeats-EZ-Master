use crate::state::{Engine, public_settings};
use serde_json::{Value, json};

pub fn describe(engine: &Engine) -> Value {
    let audition = if engine.desktop.is_some() {
        "open_in_desktop to audition"
    } else {
        "Ask the user to audition the temporary WAV in a local player; desktop is not configured"
    };
    let profiles = engine.profiles.describe();
    json!({"companion_version":env!("CARGO_PKG_VERSION"),"engine_version":mastercraft::branding::ENGINE_VERSION,
        "profiles":profiles,"profile_catalog":engine.profiles.policy(),"export_formats":["wav16","wav24","wav32f","flac16","flac24","mp3"],
        "defaults":public_settings(&mastercraft::config::Settings::default()),
        "module_ids":mastercraft::config::MODULES,
        "loudness_targets":"profiles[].targets_lufs lists each mode's requested LUFS target. An explicit target or reference loudness can replace it; gain/limiter limits can prevent reaching it. Tonal intensity does not scale the loudness target.",
        "libraries":engine.policy.inputs.iter().enumerate().map(|(index,path)|json!({"index":index,"name":path.file_name().unwrap_or_default().to_string_lossy()})).collect::<Vec<_>>(),
        "max_decoded_input_mib":engine.max_decoded_bytes / (1024*1024),"decode_timeout_seconds":engine.decode_timeout.as_secs(),
        "concurrent_audio_jobs":1,"desktop_configured":engine.desktop.is_some(),
        "studio":{"voice_cleanup":true,"spectral_repair":true,"issue_detector":true,"stem_doctor":true,"stem_mixing":true,"stem_engine":engine.stem_runtime.status(),"vst3_hosting":crate::optional_tools::plugin_host(engine).is_some()&&!engine.plugin_roots.is_empty(),"plugin_libraries":engine.plugin_roots.iter().enumerate().map(|(index,path)|json!({"index":index,"name":path.file_name().unwrap_or_default().to_string_lossy()})).collect::<Vec<_>>(),"start":"get_help with topic stems/voice/repair/studio_export/plugins; get_studio_session lists current IDs"},
        "stem_import":{"zip_import":true,"zip_inspect_tool":"inspect_stem_bundle","zip_import_tool":"import_stem_bundle","default_preferred_format":"wav","minimum":2,"maximum":null,"selection_default":"all distinct parts","kinds":mastercraft::studio::stem_mix::StemKind::ALL,"kinds_are_examples":true,"extra_parts":"Use unique canonical part_N IDs (positive decimal integer without leading zeros), e.g. part_10, part_24, part_64, for custom instruments or repeated instrument types. The kinds examples are not an exhaustive ID pool; these are identities, not neural extraction targets.","resource_limits":stem_import_limits(engine),"automatic_separation_modes":["vocal_instrumental","four_stems"]},
        "optional_tools":crate::optional_tools::policy(),
        "stem_region_edit":{"tool":"edit_stem_region","help_topic":"stems","operations":["silence","fade_in","fade_out","gain","reverse"],"time_units":"numeric seconds","gain_db_range":[-60,12],"selection":"Current bank and stable kind only; nearest-frame rounding with exclusive end. Pending mix jobs must finish first.","result":"New immutable bank and undoable temporary whole-mix preview using current controls. Original mix and other stems are preserved. Audio edits are rendered; desktop faders remain live. No arrangement, split, MIDI or final export."},
        "issue_detector":{"scan_tool":"scan_audio_issues","repair_tool":"preview_stem_doctor","help_topic":"doctor","read_only_scan":true,"kinds":["clipping","silence","possible_ending","low_mid_buildup"],"repair_actions":["declip","reduce_low_mids"],"selection_units":"numeric seconds; nearest-frame rounding with exclusive end","strength_range":[0,1],"automatic_trim":false,"models_required":false,"interpretation":"Timestamped measurements and heuristic candidates, not calibrated probabilities or listening judgments. Inspect report.thresholds, evidence, notes and truncated. Repairs are explicit temporary Studio previews; direct stem previews require apply_preview_to_stem before bank/mix changes. Export is separate."},
        "tool_count":45,"support_reports":crate::support_reports::policy(),
        "privacy":"Local stdio only. This companion does not upload audio. Your AI client may send tool requests, measurements and filenames to its model provider. No audio is sent through tool results.",
        "memory":"The input limit is decoded PCM, not total process memory. Mastering holds several working buffers. One source and one master per session; longer tracks depend on RAM.",
        "workflow":["get_help for complete examples and units","list_audio_files","load_audio and poll for source_id","analyze_audio","master_preview and poll for preview_id","optional suggest_eq then apply_eq",audition,"export_audio only when requested"],
        "help_topics":["quick_start","mastering","eq","export","troubleshooting","privacy","stems","voice","repair","doctor","studio_export","plugins","optional_tools","support_reports","recovery"]})
}

pub(crate) fn stem_import_limits(engine: &Engine) -> Value {
    json!({
        "fixed_stem_count_cap":false,
        "max_aggregate_decoded_bank_mib":mastercraft::studio_native::stem_files::DEFAULT_BANK_DECODED_BYTES / (1024 * 1024),
        "max_decoded_input_mib":engine.max_decoded_bytes / (1024 * 1024),
        "decode_timeout_seconds":engine.decode_timeout.as_secs(),
        "max_zip_archive_bytes":mastercraft::studio::stem_archive::MAX_ARCHIVE_BYTES,
        "max_expanded_zip_audio_bytes":mastercraft::studio::stem_archive::MAX_ENTRY_BYTES,
        "max_import_metadata_bytes":mastercraft::studio::stem_import_plan::MAX_IMPORT_METADATA_BYTES,
        "metadata_overhead_bytes_per_entry":mastercraft::studio::stem_import_plan::IMPORT_FILE_METADATA_OVERHEAD,
        "metadata_accounting":"Aggregate UTF-8 filename bytes plus the per-entry overhead, including ignored ZIP entries; this bounds metadata memory rather than fixing a stem count.",
        "memory":"No fixed imported stem-count cap. max_decoded_input_mib applies separately to each input (default 512 MiB); the native bank has a distinct 4 GiB aggregate decoded-audio budget. Available RAM, ZIP byte/metadata budgets and decoding timeouts still apply. Working mixes, processing and undo history need additional memory; these are not total process-memory limits."
    })
}
