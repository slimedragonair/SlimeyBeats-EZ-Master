//! Compact typed entry points for arrangement metadata and read-only inspection.
use crate::{arrangement_arguments::*, protocol::Companion};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArrangementEditRequest {
    pub operation: ArrangementEditOperation,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ArrangementEditOperation {
    Create(CreateArrangement),
    AddAssets(AddArrangementAssets),
    Apply(EditArrangement),
    Undo(ArrangementRevision),
    Redo(ArrangementRevision),
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArrangementInspectRequest {
    pub operation: ArrangementInspectOperation,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ArrangementInspectOperation {
    State {},
    Waveform(ArrangementWaveform),
    EqResponse(ArrangementEqResponse),
}

#[tool_router(router=compact_arrangement_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Edit non-destructive Tracks metadata. operation.action: create (sample_rate in Hz, default 48000 stereo; replacement needs replace_session_id), add_assets (explicit owned audio_id sources), apply (atomic typed edits, one Undo), undo, redo. Except create, require current session_id/expected_revision from arrangement_inspect state. Frames are integers, ends exclusive; clip EQ ranges use SOURCE frames, mix/lane ranges PROJECT frames; gains use dB. Undo/redo direction is fixed by action. No rendering, autoplay or export.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn arrangement_edit(
        &self,
        Parameters(request): Parameters<ArrangementEditRequest>,
    ) -> Result<Json<Value>, String> {
        match request.operation {
            ArrangementEditOperation::Create(value) => self.create_arrangement(Parameters(value)),
            ArrangementEditOperation::AddAssets(value) => {
                self.add_arrangement_assets(Parameters(value))
            }
            ArrangementEditOperation::Apply(value) => self.edit_arrangement(Parameters(value)),
            ArrangementEditOperation::Undo(value) => {
                self.undo_arrangement(Parameters(ArrangementHistoryRequest {
                    session_id: value.session_id,
                    expected_revision: value.expected_revision,
                    redo: false,
                }))
            }
            ArrangementEditOperation::Redo(value) => {
                self.undo_arrangement(Parameters(ArrangementHistoryRequest {
                    session_id: value.session_id,
                    expected_revision: value.expected_revision,
                    redo: true,
                }))
            }
        }
    }

    #[tool(
        description = "Inspect Tracks without changing audio. operation.action: state (current session/revision, asset/lane/clip/effect IDs, history and preview status), waveform (owned asset_id, SOURCE frame range with exclusive end, bins 1..16384), eq_response (complete scoped effect; frequencies in Hz and gain in dB). Waveform/EQ response require current session_id/expected_revision. Returns bounded metadata, not PCM; no job, render, apply or export.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) fn arrangement_inspect(
        &self,
        Parameters(request): Parameters<ArrangementInspectRequest>,
    ) -> Result<Json<Value>, String> {
        match request.operation {
            ArrangementInspectOperation::State {} => self.get_arrangement(),
            ArrangementInspectOperation::Waveform(value) => {
                self.get_arrangement_waveform(Parameters(value))
            }
            ArrangementInspectOperation::EqResponse(value) => {
                self.get_arrangement_eq_response(Parameters(value))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region_edit_test_support::Workspace;
    use serde_json::json;

    #[test]
    fn schemas_keep_object_roots_and_reject_direction_or_inspection_overrides() {
        for schema in [
            schemars::schema_for!(ArrangementEditRequest),
            schemars::schema_for!(ArrangementInspectRequest),
        ] {
            let schema = serde_json::to_value(schema).unwrap();
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["additionalProperties"], false);
            assert_eq!(schema["required"], json!(["operation"]));
        }
        for action in ["undo", "redo"] {
            let valid =
                json!({"operation":{"action":action,"session_id":"session","expected_revision":3}});
            serde_json::from_value::<ArrangementEditRequest>(valid.clone()).unwrap();
            let mut invalid = valid;
            invalid["operation"]["redo"] = json!(action == "undo");
            assert!(serde_json::from_value::<ArrangementEditRequest>(invalid).is_err());
        }
        for invalid in [
            json!({"action":"state"}),
            json!({"operation":{"action":"state","render":true}}),
            json!({"operation":{"action":"waveform"}}),
        ] {
            assert!(serde_json::from_value::<ArrangementInspectRequest>(invalid).is_err());
        }
    }

    #[test]
    fn fixed_undo_redo_actions_keep_revision_guards_and_do_not_start_jobs() {
        let workspace = Workspace::new();
        let companion = Companion::new(workspace.engine.clone());
        let call = |operation| {
            companion.arrangement_edit(Parameters(
                serde_json::from_value(json!({"operation":operation})).unwrap(),
            ))
        };
        let created = call(json!({"action":"create","sample_rate":8000,"channels":2}))
            .unwrap()
            .0;
        let added = call(
            json!({"action":"add_assets","session_id":created["session_id"],"expected_revision":0,
            "sources":[{"audio_id":"stems-baseline:vocals"}]}),
        )
        .unwrap()
        .0;
        let lane = added["arrangement"]["lanes"][0]["id"].clone();
        let edited = call(json!({"action":"apply","session_id":added["session_id"],"expected_revision":added["revision"],
            "edits":[{"kind":"set_lane","lane_id":lane,"gain_db":-6}]})).unwrap().0;
        let undo = json!({"action":"undo","session_id":edited["session_id"],"expected_revision":edited["revision"]});
        let undone = call(undo.clone()).unwrap().0;
        assert_eq!(undone["arrangement"]["lanes"][0]["gain_db"], 0.0);
        assert!(call(undo).is_err());
        let redone = call(json!({"action":"redo","session_id":undone["session_id"],"expected_revision":undone["revision"]})).unwrap().0;
        assert_eq!(redone["arrangement"]["lanes"][0]["gain_db"], -6.0);
        let inspected = companion
            .arrangement_inspect(Parameters(ArrangementInspectRequest {
                operation: ArrangementInspectOperation::State {},
            }))
            .unwrap()
            .0;
        assert_eq!(inspected["revision"], redone["revision"]);
        let store = workspace.engine.lock().unwrap();
        assert!(
            store.jobs.is_empty() && store.active.is_none() && store.arrangements.preview.is_none()
        );
    }
}
