//! Arrangement persistence accepts owned IDs, never caller-selected filesystem paths.
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoadArrangementProject {
    /// Opaque project_id returned by save_arrangement_project/open_arrangement_in_desktop.
    /// Retain it across restarts; it is scoped to the same configured output root.
    pub project_id: String,
    /// Required together with expected_revision when an arrangement is already open.
    /// Omit both only for an empty arrangement workspace after a fresh connection/clear.
    pub replace_session_id: Option<String>,
    pub expected_revision: Option<u64>,
}
