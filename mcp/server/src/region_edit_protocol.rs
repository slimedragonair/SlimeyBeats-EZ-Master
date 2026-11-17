//! Selected-stem time edits use the shared core schema and current-bank identity.
use crate::{protocol::Companion, region_edit_operations};
use mastercraft::studio::{region_edit::RegionEdit, stem_mix::StemKind};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditStemRegion {
    /// Current stem_set.stem_set_id from get_studio_session; stale banks are rejected.
    pub stem_set_id: String,
    /// Stable kind from the current bank's controls (not a display name or audio_id).
    pub kind: StemKind,
    /// Numeric seconds within this stem. End is exclusive after nearest-frame rounding.
    pub edit: RegionEdit,
}

#[tool_router(router=region_edit_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Render a time selection edit into one current stem: silence, fade_in, fade_out, gain {db:-60..12}, or reverse. Discover stem_set_id and stable kind with get_studio_session. edit has numeric start_seconds, end_seconds and operation:{kind,...}; times must select at least one frame within the stem. Wait for pending mix_stems jobs first. Replaces only that stem in a new immutable bank, preserving length/rate/channels, other stems, original mix and existing mix controls. Renders an undoable temporary WHOLE MIX through those controls; faders remain live in the desktop. Poll get_job_status for new stem_set_id, studio_preview_id and all assets/IDs. No export, clip arrangement, split or MIDI; get_help(topic=stems) has examples.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn edit_stem_region(
        &self,
        Parameters(a): Parameters<EditStemRegion>,
    ) -> Result<Json<Value>, String> {
        let check = a.clone();
        self.response(self.engine.start(
            "edit_stem_region",
            move |s| {
                region_edit_operations::validate(s, &check.stem_set_id, check.kind, &check.edit)
            },
            move |ctx| region_edit_operations::render(ctx, a.stem_set_id, a.kind, a.edit),
        ))
    }
}

#[cfg(test)]
#[path = "region_edit_protocol_tests.rs"]
mod tests;
