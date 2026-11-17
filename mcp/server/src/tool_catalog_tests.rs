use crate::{arguments::Arguments, protocol::Companion, state::Engine, tool_catalog::*};
use clap::Parser;
use std::collections::BTreeSet;

fn fixture() -> (tempfile::TempDir, std::sync::Arc<Engine>) {
    let root = tempfile::tempdir().unwrap();
    let args = Arguments::try_parse_from([
        "slimeybeats-mcp",
        "--input-root",
        root.path().to_str().unwrap(),
        "--output-root",
        root.path().join("exports").to_str().unwrap(),
    ])
    .unwrap();
    assert_eq!(args.tool_profile, ToolProfile::Compact);
    let engine = Engine::new(&args).unwrap();
    (root, engine)
}

#[test]
fn main_discovery_is_complete_bounded_and_grouped_without_legacy_leakage() {
    let (_root, engine) = fixture();
    let main = Companion::new(engine.clone());
    let tools = main.visible_tools();
    let expected = COMPACT_TOOL_NAMES.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(expected.len(), 33);
    assert!(tools.len() < 35);
    assert_eq!(
        tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<BTreeSet<_>>(),
        expected
    );
    for tool in &tools {
        assert_eq!(
            tool.input_schema
                .get("type")
                .and_then(serde_json::Value::as_str),
            Some("object"),
            "{}",
            tool.name
        );
        assert!(tool.description.as_ref().is_some_and(|d| d.len() > 35));
    }
    let groups = groups();
    let grouped = groups
        .as_object()
        .unwrap()
        .values()
        .flat_map(|v| v.as_array().unwrap())
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(grouped.len(), 33);
    assert_eq!(grouped.into_iter().collect::<BTreeSet<_>>(), expected);
    let caps = main.get_capabilities().0;
    assert_eq!(caps["tool_count"], 33);
    assert_eq!(caps["tool_profile"], "compact");
    assert!(!caps.to_string().contains("save_arrangement_project"));
    let legacy = Companion::with_tool_profile(engine, ToolProfile::Legacy);
    assert_eq!(legacy.visible_tools().len(), LEGACY_TOOL_COUNT);
    assert_eq!(legacy.get_capabilities().0["tool_count"], LEGACY_TOOL_COUNT);
    assert!(
        !legacy
            .visible_tools()
            .iter()
            .any(|tool| tool.name == "project")
    );
}

#[test]
fn static_help_calls_only_use_main_tools_and_preserve_range_and_consent_guidance() {
    fn calls(value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(tool) = map
                    .get("tool")
                    .and_then(serde_json::Value::as_str)
                    .filter(|_| map.contains_key("arguments"))
                {
                    assert!(
                        COMPACT_TOOL_NAMES.contains(&tool),
                        "Unadvertised help tool {tool}"
                    );
                    assert!(
                        map.get("arguments").is_some(),
                        "Missing arguments for {tool}"
                    );
                }
                map.values().for_each(calls);
            }
            serde_json::Value::Array(values) => values.iter().for_each(calls),
            _ => {}
        }
    }
    use crate::help::Topic;
    for topic in [
        Topic::QuickStart,
        Topic::Mastering,
        Topic::Eq,
        Topic::Export,
        Topic::Troubleshooting,
        Topic::Privacy,
        Topic::Stems,
        Topic::Voice,
        Topic::Repair,
        Topic::Doctor,
        Topic::StudioExport,
        Topic::Plugins,
        Topic::OptionalTools,
        Topic::SupportReports,
        Topic::Recovery,
        Topic::Hybrid,
        Topic::Arrangement,
    ] {
        let help = crate::compact_help::get(topic);
        calls(&help);
        assert_eq!(help["tool_count"], 33);
    }
    let arrangement = crate::compact_help::get(Topic::Arrangement).to_string();
    assert!(arrangement.contains("SOURCE frames"));
    assert!(arrangement.contains("PROJECT frames"));
    let setup = crate::compact_help::get(Topic::OptionalTools).to_string();
    assert!(setup.contains("AFTER owner agreement"));
    assert!(setup.contains("allow_download:true"));
    let studio_export = crate::compact_help::get(Topic::StudioExport);
    assert!(
        studio_export["arrangement_workspace"]["history"]
            .as_str()
            .unwrap()
            .contains("operation.action=undo or redo")
    );
    assert!(!studio_export.to_string().contains("redo:true"));
    for topic in [Topic::Doctor, Topic::Voice] {
        let help = crate::compact_help::get(topic).to_string();
        assert!(
            help.contains("studio.assets[]"),
            "Studio example must name the nested result path"
        );
    }
}

#[test]
fn config_round_trip_preserves_explicit_profile_and_unknown_profiles_are_rejected() {
    let (root, _) = fixture();
    let input = root.path().to_str().unwrap();
    let output = root.path().join("exports");
    let output = output.to_str().unwrap();
    for profile in ["compact", "legacy"] {
        let args = Arguments::try_parse_from([
            "server",
            "--input-root",
            input,
            "--output-root",
            output,
            "--tool-profile",
            profile,
        ])
        .unwrap();
        let config = args.client_config().unwrap();
        let values = config["mcpServers"]["slimeybeats"]["args"]
            .as_array()
            .unwrap();
        let index = values.iter().position(|v| v == "--tool-profile").unwrap();
        assert_eq!(values[index + 1], profile);
    }
    assert!(
        Arguments::try_parse_from([
            "server",
            "--input-root",
            input,
            "--output-root",
            output,
            "--tool-profile",
            "everything"
        ])
        .is_err()
    );
}

#[test]
fn catalog_guidance_does_not_rename_owner_profile_or_library_identities() {
    let original = serde_json::json!({
        "profiles":[{"id":"export_audio","name":"create_arrangement"}],
        "libraries":[{"name":"get_arrangement"}],
        "studio":{"plugin_libraries":[{"name":"clean_voice"}],"start":"get_studio_session lists current IDs"},
        "stem_import":{"zip_import_tool":"import_stem_bundle"}
    });
    let mapped = crate::compact_guidance::capabilities(original.clone());
    assert_eq!(mapped["profiles"], original["profiles"]);
    assert_eq!(mapped["libraries"], original["libraries"]);
    assert_eq!(
        mapped["studio"]["plugin_libraries"],
        original["studio"]["plugin_libraries"]
    );
    assert_eq!(mapped["stem_import"]["zip_import_tool"], "stem_import");
    assert!(
        mapped["studio"]["start"]
            .as_str()
            .unwrap()
            .contains("get_session.studio")
    );
}
