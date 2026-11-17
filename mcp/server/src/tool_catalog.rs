//! Stable discovery profiles. Compatibility routes never inflate main discovery.
use clap::ValueEnum;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum ToolProfile {
    #[default]
    Compact,
    Legacy,
}
impl ToolProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Legacy => "legacy",
        }
    }
}

pub const COMPACT_TOOL_NAMES: &[&str] = &[
    "get_capabilities",
    "get_help",
    "list_audio_files",
    "get_session",
    "get_job_status",
    "cancel_job",
    "load_audio",
    "reference",
    "analyze",
    "suggest_eq",
    "master_preview",
    "apply_eq",
    "clear_session",
    "export_result",
    "open_desktop",
    "stem_import",
    "separate_stems",
    "mix_stems",
    "edit_stem_region",
    "apply_preview_to_stem",
    "process_audio",
    "scan_audio_issues",
    "studio_history",
    "use_studio_for_mastering",
    "arrangement_edit",
    "arrangement_inspect",
    "suggest_arrangement_eq",
    "render_arrangement",
    "project",
    "optional_tool_status",
    "setup_optional_tool",
    "plugin",
    "support_report",
];
pub const LEGACY_TOOL_COUNT: usize = 58;

/// Maps old calls to typed main operations. None means an unchanged main tool.
pub fn replacement(name: &str) -> Option<(&'static str, Option<&'static str>)> {
    Some(match name {
        "load_reference" => ("reference", Some("load")),
        "clear_reference" => ("reference", Some("clear")),
        "analyze_audio" => ("analyze", Some("mastering")),
        "analyze_studio_audio" => ("analyze", Some("studio")),
        "get_studio_session" | "get_arrangement" => ("get_session", None),
        "export_audio" => ("export_result", Some("mastering")),
        "export_studio_audio" => ("export_result", Some("studio")),
        "export_arrangement" => ("export_result", Some("arrangement")),
        "export_stems" => ("export_result", Some("raw_stems")),
        "open_in_desktop" => ("open_desktop", Some("mastering")),
        "open_studio_in_desktop" => ("open_desktop", Some("studio")),
        "open_arrangement_in_desktop" => ("open_desktop", Some("arrangement")),
        "inspect_stem_bundle" => ("stem_import", Some("inspect_zip")),
        "import_stem_bundle" => ("stem_import", Some("zip")),
        "import_stems" => ("stem_import", Some("files")),
        "clean_voice" => ("process_audio", Some("voice_cleanup")),
        "repair_audio" => ("process_audio", Some("spectral_repair")),
        "preview_stem_doctor" => ("process_audio", Some("stem_doctor")),
        "undo_studio_edit" => ("studio_history", Some("undo")),
        "redo_studio_edit" => ("studio_history", Some("redo")),
        "create_arrangement" => ("arrangement_edit", Some("create")),
        "add_arrangement_assets" => ("arrangement_edit", Some("add_assets")),
        "edit_arrangement" => ("arrangement_edit", Some("apply")),
        "undo_arrangement" => ("arrangement_edit", Some("undo")),
        "get_arrangement_waveform" => ("arrangement_inspect", Some("waveform")),
        "get_arrangement_eq_response" => ("arrangement_inspect", Some("eq_response")),
        "save_arrangement_project" => ("project", Some("save_arrangement")),
        "load_arrangement_project" => ("project", Some("open_arrangement")),
        "save_studio_project" => ("project", Some("save_studio")),
        "get_optional_tool_status" | "get_stem_engine_status" => ("optional_tool_status", None),
        "prepare_optional_tool" => ("setup_optional_tool", Some("component")),
        "install_stem_engine" => ("setup_optional_tool", Some("stem_engine")),
        "list_plugins" => ("plugin", Some("list")),
        "probe_plugin" => ("plugin", Some("probe")),
        "render_plugin" => ("plugin", Some("render")),
        "prepare_support_report" => ("support_report", Some("preview")),
        "save_support_report" => ("support_report", Some("save")),
        _ => return None,
    })
}

pub fn groups() -> Value {
    json!({
        "start_here":["get_capabilities","get_help","list_audio_files","get_session"],
        "jobs":["get_job_status","cancel_job"],
        "mastering":["load_audio","reference","analyze","suggest_eq","master_preview","apply_eq","clear_session"],
        "delivery":["export_result","open_desktop","project"],
        "stems_and_repair":["stem_import","separate_stems","mix_stems","edit_stem_region","apply_preview_to_stem","process_audio","scan_audio_issues","studio_history","use_studio_for_mastering"],
        "tracks":["arrangement_edit","arrangement_inspect","suggest_arrangement_eq","render_arrangement"],
        "optional_and_support":["optional_tool_status","setup_optional_tool","plugin","support_report"]
    })
}

/// Only static guidance passes here; never rewrite caller filenames or audio data.
pub fn guidance(text: &str) -> String {
    // Split on identifier boundaries so overlapping names cannot rewrite each other.
    let mut output = String::new();
    let mut identifier = String::new();
    fn flush(output: &mut String, identifier: &mut String) {
        if identifier == "get_studio_session" {
            output.push_str("get_session.studio");
        } else if identifier == "get_arrangement" {
            output.push_str("get_session.arrangement");
        } else if let Some((name, action)) = replacement(identifier) {
            output.push_str(name);
            if let Some(action) = action {
                output.push_str(&format!(" (operation.action={action})"));
            }
        } else {
            output.push_str(identifier);
        }
        identifier.clear();
    }
    for character in text.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            identifier.push(character);
        } else {
            flush(&mut output, &mut identifier);
            output.push(character);
        }
    }
    flush(&mut output, &mut identifier);
    output
}
