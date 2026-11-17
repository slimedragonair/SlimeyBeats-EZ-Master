//! Typed arrangement tools share core edits/scopes, with explicit audio jobs.
use crate::{
    arrangement_arguments::*,
    arrangement_commands as commands, arrangement_operations as operations,
    jobs::{ContextData, Product},
    protocol::Companion,
};
use mastercraft::studio::arrangement_session::ArrangementSession;
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use serde_json::Value;
use std::sync::Arc;

impl Companion {
    fn arrangement_job(
        &self,
        kind: &str,
        id: String,
        revision: u64,
        work: impl FnOnce(ContextData, Arc<ArrangementSession>) -> anyhow::Result<Product>
        + Send
        + 'static,
    ) -> Result<Json<Value>, String> {
        let selected = self
            .engine
            .lock()
            .and_then(|store| store.arrangements.selected(&id, revision));
        let session = match selected {
            Ok(value) => value,
            Err(error) => return self.response(Err(error)),
        };
        self.response(self.engine.start_checked(
            kind,
            move |store| store.arrangements.selected(&id, revision).map(|_| ()),
            move |ctx| work(ctx, session),
        ))
    }
}

#[cfg(test)]
#[path = "arrangement_tests.rs"]
mod tests;

#[tool_router(router=arrangement_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Create a non-destructive clip arrangement, default 48000 Hz stereo. Existing workspace replacement requires its explicit replace_session_id. Does not render, export, modify aligned banks or master audio. Discover instructions with get_help(topic=arrangement).",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn create_arrangement(
        &self,
        Parameters(a): Parameters<CreateArrangement>,
    ) -> Result<Json<Value>, String> {
        self.response(commands::create(&self.engine, a))
    }
    #[tool(
        description = "Read current arrangement session_id, revision, source/lane/clip/effect IDs, labels, exact frame geometry, Undo/Redo and preview staleness. Returns metadata only; never PCM or supplied paths.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) fn get_arrangement(&self) -> Result<Json<Value>, String> {
        self.response(commands::read(&self.engine))
    }
    #[tool(
        description = "Append one new lane per explicitly selected owned audio_id from get_studio_session. Starts each full immutable source at project frame0; differing durations and mono-to-stereo are allowed. Sample rates must match; conform explicitly first. One Undo, no rendering/export.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn add_arrangement_assets(
        &self,
        Parameters(a): Parameters<AddArrangementAssets>,
    ) -> Result<Json<Value>, String> {
        self.response(commands::add(&self.engine, a))
    }
    #[tool(
        description = "Apply an atomic shared-core arrangement Edit batch, one Undo: add/move/trim/split/duplicate/delete clips, lanes/controls, scoped EQ. Use explicit current IDs and expected_revision. Frames are integers, end exclusive. Clip EQ range uses SOURCE frames; Mix/Lane use PROJECT frames. Exclude only replaced effect during suggestions; set_eq upserts its stable ID. No PCM rewrite/render/export.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn edit_arrangement(
        &self,
        Parameters(a): Parameters<EditArrangement>,
    ) -> Result<Json<Value>, String> {
        self.response(commands::edit(&self.engine, a))
    }
    #[tool(
        description = "Undo the last arrangement metadata transaction; redo:true restores the next one. Immutable source assets remain owned. No render or export; current preview may become stale.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn undo_arrangement(
        &self,
        Parameters(a): Parameters<ArrangementHistoryRequest>,
    ) -> Result<Json<Value>, String> {
        self.response(commands::history(&self.engine, a))
    }
    #[tool(
        description = "Read a bounded waveform envelope from an owned arrangement asset_id. range is immutable SOURCE frames, end exclusive; bins1..16384 includes both channels. Does not render the timeline or send PCM.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) fn get_arrangement_waveform(
        &self,
        Parameters(a): Parameters<ArrangementWaveform>,
    ) -> Result<Json<Value>, String> {
        self.response((|| {
            let session = self
                .engine
                .lock()?
                .arrangements
                .selected(&a.session_id, a.expected_revision)?;
            Ok(serde_json::to_value(session.waveform(
                &a.asset_id,
                a.range,
                a.bins,
            )?)?)
        })())
    }
    #[tool(
        description = "Compute the actual shared Rust frequency response of a complete scoped EQ effect. Returns frequencies Hz and gain_db; it does not apply the effect or render PCM.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) fn get_arrangement_eq_response(
        &self,
        Parameters(a): Parameters<ArrangementEqResponse>,
    ) -> Result<Json<Value>, String> {
        self.response((|| {
            let session = self
                .engine
                .lock()?
                .arrangements
                .selected(&a.session_id, a.expected_revision)?;
            Ok(serde_json::to_value(session.eq_response(&a.effect)?)?)
        })())
    }
    #[tool(
        description = "Explicit analysis job for scoped EQ suggestions. target requires mix or exact lane_id/clip_id; filenames never choose a target. Clip ranges are SOURCE frames, Mix/Lane PROJECT frames. Lane/Clip analysis ignores mute/solo but retains appropriate routed FX and current gains. exclude_eq_id removes ONLY the effect being replaced. Poll get_job_status for bands/notes/high_frequency; nothing is applied.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) fn suggest_arrangement_eq(
        &self,
        Parameters(a): Parameters<SuggestArrangementEq>,
    ) -> Result<Json<Value>, String> {
        self.arrangement_job(
            "suggest_arrangement_eq",
            a.session_id.clone(),
            a.expected_revision,
            move |ctx, session| operations::suggest(ctx, session, a),
        )
    }
    #[tool(
        description = "Explicitly render a temporary full arrangement preview through the same core frame player, scoped EQ, gains and final peak guard. Poll get_job_status for the relative preview path; this does not start playback or export. Metadata edits never invoke this job automatically.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn render_arrangement(
        &self,
        Parameters(a): Parameters<ArrangementRevision>,
    ) -> Result<Json<Value>, String> {
        self.arrangement_job(
            "render_arrangement",
            a.session_id,
            a.expected_revision,
            operations::render,
        )
    }
    #[tool(
        description = "Explicit full-precision native arrangement export in the selected existing export format. Requires current session_id/revision and new filename stem. Shared core renders scoped effects once, then existing encoder verifies output. Poll get_job_status. Existing output files are preserved; original sources and arrangement metadata remain unchanged.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    pub(crate) fn export_arrangement(
        &self,
        Parameters(a): Parameters<ExportArrangement>,
    ) -> Result<Json<Value>, String> {
        self.arrangement_job(
            "export_arrangement",
            a.session_id.clone(),
            a.expected_revision,
            move |ctx, session| operations::export(ctx, session, a),
        )
    }
}
