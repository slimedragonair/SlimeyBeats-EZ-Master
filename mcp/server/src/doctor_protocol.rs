//! Self-contained discovery for issue reports and conservative Studio repairs.
use crate::{doctor_operations, protocol::Companion};
use mastercraft::studio::doctor::RepairRequest;
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScanAudioIssues {
    /// Loaded source_id or current stem/Studio preview audio_id from get_studio_session.
    pub audio_id: String,
}

#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewStemDoctor {
    /// Same current audio ID that was scanned; refresh IDs after every edit or undo.
    pub audio_id: String,
    /// Explicit action, selection in seconds and strength 0..1. No automatic repair-all.
    pub repair: RepairRequest,
}

#[tool_router(router=doctor_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Scan a loaded source, current stem or Studio preview for timestamped full-scale clipping candidates, quiet regions, possible endings followed by resumed music, and possible low-mid buildup. Discover audio_id with get_studio_session or use the loaded source_id. Poll get_job_status; result.report contains thresholds, issues with evidence, optional repair actions and limitations. Read-only: no audio edit, undo step, final export, model download or upload. These are measured indicators and tonal hypotheses, not a verdict on sound quality. Never trim or repair automatically. get_help(topic=doctor) documents examples and units.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn scan_audio_issues(
        &self,
        Parameters(a): Parameters<ScanAudioIssues>,
    ) -> Result<Json<Value>, String> {
        let id = a.audio_id.clone();
        self.response(self.engine.start(
            "scan_audio_issues",
            move |s| s.studio_audio(&id).map(|_| ()),
            move |ctx| doctor_operations::scan(ctx, a.audio_id),
        ))
    }

    #[tool(
        description = "Explicitly prepare a conservative Stem Doctor repair as an undoable temporary Studio preview. First scan_audio_issues; use its same current audio_id and issue's supported repair, start_seconds and end_seconds. repair:{action:declip|reduce_low_mids,start_seconds:number,end_seconds:number,strength:number 0..1}; 0 bypasses, 0.5 is a gentle starting point. Numeric times select at least one frame within the audio. Declip only handles eligible short full-scale runs; low-mid reduction is bounded gentle EQ, not instrument reconstruction. Poll for a new studio_preview_id, notes and changed_samples; scan/audition that result and undo if needed. Source and outside-region samples remain unchanged. Direct stem previews require apply_preview_to_stem to affect the bank/mix. No automatic trim, repair-all, model download, mastering change or final export. get_help(topic=doctor) has the full workflow.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn preview_stem_doctor(
        &self,
        Parameters(a): Parameters<PreviewStemDoctor>,
    ) -> Result<Json<Value>, String> {
        let check = a.clone();
        self.response(self.engine.start(
            "preview_stem_doctor",
            move |s| {
                let target = s.studio_audio(&check.audio_id)?;
                check.repair.validate(&target.audio)
            },
            move |ctx| doctor_operations::preview(ctx, a.audio_id, a.repair),
        ))
    }
}

#[cfg(test)]
#[path = "doctor_protocol_tests.rs"]
mod tests;
