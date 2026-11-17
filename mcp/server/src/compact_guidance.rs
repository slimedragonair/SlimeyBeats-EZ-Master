//! Adapt static legacy guidance and examples to the compact catalog, never user data.
use crate::tool_catalog;
use serde_json::{Map, Value, json};

pub fn help_value(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(help_value),
        Value::Object(object) => {
            let old = object
                .get("tool")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if old.as_deref() == Some("get_studio_session") {
                for key in ["read", "then", "result"] {
                    if let Some(Value::String(text)) = object.get_mut(key) {
                        *text = text
                            .replace("assets[]", "studio.assets[]")
                            .replace("stem_set.stem_set_id", "studio.stem_set.stem_set_id")
                            .replace(
                                "preview.studio_preview_id",
                                "studio.preview.studio_preview_id",
                            );
                    }
                }
            }
            if let Some((name, action)) = old.as_deref().and_then(tool_catalog::replacement) {
                object.insert("tool".into(), name.into());
                if let Some(action) = action {
                    let mut arguments = object.remove("arguments").unwrap_or_else(|| json!({}));
                    let action = if old.as_deref() == Some("undo_arrangement") {
                        if arguments
                            .as_object_mut()
                            .and_then(|a| a.remove("redo"))
                            .and_then(|v| v.as_bool())
                            == Some(true)
                        {
                            "redo"
                        } else {
                            "undo"
                        }
                    } else {
                        action
                    };
                    if old.as_deref() == Some("install_stem_engine") {
                        // Documentation must not manufacture download consent.
                        arguments["allow_download"] = false.into();
                    }
                    let mut operation = arguments.as_object().cloned().unwrap_or_default();
                    operation.insert("action".into(), action.into());
                    object.insert("arguments".into(), json!({"operation":operation}));
                } else if old.as_deref() == Some("get_stem_engine_status") {
                    object.insert("arguments".into(), json!({}));
                }
            }
            for (key, item) in object {
                // A tool name is machine-readable, not a prose annotation.
                if key != "tool" {
                    help_value(item);
                }
            }
        }
        Value::String(text) => *text = tool_catalog::guidance(text),
        _ => {}
    }
}

fn policy_value(value: &mut Value) {
    match value {
        Value::String(text) => {
            *text = if let Some((name, _)) = tool_catalog::replacement(text) {
                name.to_owned()
            } else {
                tool_catalog::guidance(text)
            };
        }
        Value::Array(items) => items.iter_mut().for_each(policy_value),
        Value::Object(object) => object.values_mut().for_each(policy_value),
        _ => {}
    }
}

pub fn capabilities(mut value: Value) -> Value {
    // Catalog profiles, library labels and plugin-root names are owner data.
    // Translate only policy/guidance fields built by the companion itself.
    for key in [
        "stem_import",
        "optional_tools",
        "stem_region_edit",
        "issue_detector",
        "support_reports",
        "arrangement",
    ] {
        if let Some(policy) = value.get_mut(key) {
            policy_value(policy);
        }
    }
    if let Some(start) = value
        .get_mut("studio")
        .and_then(|studio| studio.get_mut("start"))
    {
        policy_value(start);
    }
    value["tool_count"] = tool_catalog::COMPACT_TOOL_NAMES.len().into();
    value["tool_groups"] = tool_catalog::groups();
    value["grouped_arguments"] = json!({"shape":{"operation":{"action":"<one advertised action>","typed_fields":"Use that action's schema"}},
        "selection":"Choose the smallest relevant tool group. Each operation has a strict typed schema; there is no arbitrary tool-name dispatcher.",
        "session_views":"get_session returns mastering source/master at the root, Studio IDs under studio, and independent Tracks IDs/revision under arrangement.",
        "compatibility":"Original tool names remain callable as compatibility aliases but are not advertised in this compact catalog. --tool-profile legacy advertises the original 58 tools and guidance; it adds no audio capability."});
    value["stem_import"]["zip_inspect_action"] = "inspect_zip".into();
    value["stem_import"]["zip_import_action"] = "zip".into();
    value["issue_detector"]["repair_action"] = "stem_doctor".into();
    value["arrangement"]["projects"]["save_action"] = "save_arrangement".into();
    value["arrangement"]["projects"]["load_action"] = "open_arrangement".into();
    value["arrangement"]["projects"]["desktop_action"] = "arrangement".into();
    value["workflow"] = json!([
        "get_help(topic=quick_start)",
        "list_audio_files",
        "load_audio and poll for source_id",
        "analyze operation.action=mastering",
        "master_preview and poll for preview_id",
        "optional suggest_eq, review, apply_eq",
        "open_desktop only for requested audition",
        "export_result only when saving/exporting is requested"
    ]);
    value
}

pub fn call(tool: &str, arguments: Value) -> Value {
    let mut object = Map::new();
    object.insert("tool".into(), tool.into());
    object.insert("arguments".into(), arguments);
    Value::Object(object)
}

pub fn schema_descriptions(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, item) in object {
                if key == "description" {
                    if let Some(text) = item.as_str() {
                        *item = tool_catalog::guidance(text).into();
                    }
                } else {
                    schema_descriptions(item);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(schema_descriptions),
        _ => {}
    }
}
