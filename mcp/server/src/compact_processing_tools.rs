//! Grouped processing/history routes retain the legacy validation and job model.
use crate::{
    doctor_protocol::PreviewStemDoctor,
    protocol::Companion,
    studio_protocol::{RepairRequest, VoiceRequest},
};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProcessAudioRequest {
    pub operation: ProcessAudioOperation,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ProcessAudioOperation {
    VoiceCleanup(VoiceRequest),
    SpectralRepair(RepairRequest),
    StemDoctor(PreviewStemDoctor),
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct StudioHistoryRequest {
    pub operation: StudioHistoryOperation,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum StudioHistoryOperation {
    Undo {},
    Redo {},
}

#[tool_router(router=compact_processing_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "operation.action: voice_cleanup | spectral_repair | stem_doctor. Supply a current audio_id from get_studio_session (loaded source, direct stem or Studio preview); never select audio by filename. voice_cleanup accepts typed settings: highpass_hz 0 or 20..300 Hz, noise_reduction/deess 0..1 and documented optional controls; it does not separate speech. spectral_repair requires selection:{start_seconds,end_seconds,low_hz,high_hz,reduction_db,mode}; times are numeric seconds, Hz within Nyquist, reduction -60..0 dB, mode attenuate|heal. stem_doctor follows a read-only issue scan and requires an explicitly requested repair:{action:declip|reduce_low_mids,start_seconds,end_seconds,strength:0..1}; never trim/repair automatically. All actions preserve immutable source audio and create undoable temporary Studio previews. Poll get_job_status using job_id for studio_preview_id. Direct stem edits require explicit apply_preview_to_stem with current bank/preview IDs before affecting the bank/mix. Export is separate and only on user request; get_help(topic=voice|repair|doctor) gives defaults/limits.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn process_audio(
        &self,
        Parameters(request): Parameters<ProcessAudioRequest>,
    ) -> Result<Json<Value>, String> {
        match request.operation {
            ProcessAudioOperation::VoiceCleanup(arguments) => {
                self.clean_voice(Parameters(arguments))
            }
            ProcessAudioOperation::SpectralRepair(arguments) => {
                self.repair_audio(Parameters(arguments))
            }
            ProcessAudioOperation::StemDoctor(arguments) => {
                self.preview_stem_doctor(Parameters(arguments))
            }
        }
    }

    #[tool(
        description = "operation.action: undo | redo, with no other action fields. Restore a retained legacy Studio cleanup/aligned-bank edit and its audio/controls; inspect get_studio_session for availability/current IDs. Poll the returned job_id with get_job_status, then refresh IDs before further edits. A new edit clears redo; immutable source files and prior final exports are preserved. This history is separate from Tracks clip/scoped-EQ arrangement history; use the Tracks tools and get_help(topic=arrangement) for those edits. No automatic apply_preview_to_stem, export or file deletion.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn studio_history(
        &self,
        Parameters(request): Parameters<StudioHistoryRequest>,
    ) -> Result<Json<Value>, String> {
        match request.operation {
            StudioHistoryOperation::Undo {} => self.undo_studio_edit(),
            StudioHistoryOperation::Redo {} => self.redo_studio_edit(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region_edit_test_support::Workspace;
    use serde_json::json;
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };

    fn finished(workspace: &Workspace, submitted: Json<Value>) -> Value {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let job = workspace
                .engine
                .job(submitted.0["job_id"].as_str().unwrap())
                .unwrap();
            if job["status"] != "running" {
                assert_eq!(job["status"], "completed", "{job}");
                return job;
            }
            assert!(Instant::now() < deadline, "{job}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn processing_and_history_schemas_require_strict_object_actions() {
        let tools = Companion::compact_processing_router().list_all();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "process_audio");
        assert_eq!(tools[1].name, "studio_history");
        assert!(
            tools
                .iter()
                .all(|tool| tool.input_schema["type"] == "object")
        );
        for schema in [
            serde_json::to_value(schemars::schema_for!(ProcessAudioRequest)).unwrap(),
            serde_json::to_value(schemars::schema_for!(StudioHistoryRequest)).unwrap(),
        ] {
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["additionalProperties"], false);
        }
        for arguments in [
            json!({"operation":{"action":"voice_cleanup","audio_id":"current","settings":{"export":true}}}),
            json!({"operation":{"action":"voice_cleanup","audio_id":"current","filename":"voice.wav"}}),
            json!({"operation":{"action":"spectral_repair","audio_id":"current"}}),
            json!({"operation":{"action":"stem_doctor","audio_id":"current","repair":{"action":"remove_silence","start_seconds":0,"end_seconds":1,"strength":0.5}}}),
            json!({"operation":{"action":"clean_voice","audio_id":"current"}}),
        ] {
            assert!(serde_json::from_value::<ProcessAudioRequest>(arguments).is_err());
        }
        for action in ["undo", "redo"] {
            serde_json::from_value::<StudioHistoryRequest>(json!({"operation":{"action":action}}))
                .unwrap();
            assert!(
                serde_json::from_value::<StudioHistoryRequest>(
                    json!({"operation":{"action":action,"audio_id":"current"}})
                )
                .is_err()
            );
        }
        assert!(
            serde_json::from_value::<StudioHistoryRequest>(json!({"operation":{"action":"clear"}}))
                .is_err()
        );
    }

    #[test]
    fn processing_retains_validation_and_stale_id_guards_before_job_reservation() {
        let workspace = Workspace::new();
        let companion = Companion::new(workspace.engine.clone());
        let before = workspace.engine.session().unwrap();
        for operation in [
            json!({"action":"voice_cleanup","audio_id":"stems-baseline:part_64","settings":{"highpass_hz":5}}),
            json!({"action":"spectral_repair","audio_id":"stems-baseline:part_64","selection":{"start_seconds":0.01,"end_seconds":0.1,"low_hz":0,"high_hz":5000,"reduction_db":-3,"mode":"attenuate"}}),
            json!({"action":"stem_doctor","audio_id":"stems-baseline:part_64","repair":{"action":"reduce_low_mids","start_seconds":0.01,"end_seconds":0.1,"strength":1.1}}),
            json!({"action":"voice_cleanup","audio_id":"stems-stale:part_64"}),
        ] {
            let request = serde_json::from_value(json!({"operation":operation})).unwrap();
            assert!(companion.process_audio(Parameters(request)).is_err());
        }
        for operation in [
            StudioHistoryOperation::Undo {},
            StudioHistoryOperation::Redo {},
        ] {
            assert!(
                companion
                    .studio_history(Parameters(StudioHistoryRequest { operation }))
                    .is_err()
            );
        }
        assert_eq!(workspace.engine.session().unwrap(), before);
        let store = workspace.engine.lock().unwrap();
        assert!(store.jobs.is_empty() && store.active.is_none());
    }

    #[test]
    fn processing_actions_publish_previews_and_history_restores_without_applying_stems() {
        for (operation, kind) in [
            (
                json!({"action":"voice_cleanup","audio_id":"stems-baseline:part_64","settings":{"highpass_hz":0,"noise_reduction":0,"deess":0,"hold_loudness":false,"protect_peaks":false}}),
                "clean_voice",
            ),
            (
                json!({"action":"spectral_repair","audio_id":"stems-baseline:part_64","selection":{"start_seconds":0.01,"end_seconds":0.1,"low_hz":0,"high_hz":2000,"reduction_db":0,"mode":"attenuate"}}),
                "repair_audio",
            ),
            (
                json!({"action":"stem_doctor","audio_id":"stems-baseline:part_64","repair":{"action":"reduce_low_mids","start_seconds":0.01,"end_seconds":0.1,"strength":0}}),
                "preview_stem_doctor",
            ),
        ] {
            let workspace = Workspace::new();
            let companion = Companion::new(workspace.engine.clone());
            let request = serde_json::from_value(json!({"operation":operation})).unwrap();
            let job = finished(
                &workspace,
                companion.process_audio(Parameters(request)).unwrap(),
            );
            assert_eq!(job["kind"], kind);
            let preview_id = job["result"]["studio_preview_id"]
                .as_str()
                .unwrap()
                .to_owned();
            assert_ne!(preview_id, "studio-baseline");
            {
                let store = workspace.engine.lock().unwrap();
                assert!(Arc::ptr_eq(
                    store.session.studio.bank.as_ref().unwrap(),
                    &workspace.bank
                ));
            }
            let undo = finished(
                &workspace,
                companion
                    .studio_history(Parameters(StudioHistoryRequest {
                        operation: StudioHistoryOperation::Undo {},
                    }))
                    .unwrap(),
            );
            assert_eq!(
                undo["result"]["preview"]["studio_preview_id"],
                "studio-baseline"
            );
            let redo = finished(
                &workspace,
                companion
                    .studio_history(Parameters(StudioHistoryRequest {
                        operation: StudioHistoryOperation::Redo {},
                    }))
                    .unwrap(),
            );
            assert_eq!(
                redo["result"]["preview"]["studio_preview_id"].as_str(),
                Some(preview_id.as_str())
            );
        }
    }
}
