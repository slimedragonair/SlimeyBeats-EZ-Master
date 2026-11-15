//! Connection-local immutable reports; independent of jobs and audio revisions.
use crate::{
    file_policy,
    state::Engine,
    support_report_protocol::{PrepareSupportReport, SaveSupportReport},
};
use anyhow::{Context, Result, ensure};
use mastercraft::support_report::{self, Activity, Counts, Report, ReportContext, ReportDraft};
use serde_json::{Value, json};
use std::sync::Mutex;

pub const CLIENT_PRIVACY_NOTICE: &str = "Your AI client may send tool arguments and returned report text to its model provider before report review or saving. Filtering here cannot retract anything already sent to that client. Use only non-sensitive descriptions. Typed text may remain sensitive despite filtering. Ask the owner to review every line; never autofill raw errors, logs, paths or names. Saving is local only and never posts an issue or uploads a report.";

#[derive(Default)]
pub(crate) struct ReviewStore {
    sequence: u64,
    current: Option<ReviewedReport>,
}

struct ReviewedReport {
    id: String,
    report: Report,
}

pub(crate) fn prepare(
    engine: &Engine,
    reviews: &Mutex<ReviewStore>,
    request: PrepareSupportReport,
) -> Result<Value> {
    let draft = ReportDraft::new(request.category, &request.title, &request.details)?;
    let mut context = ReportContext::for_mcp(env!("CARGO_PKG_VERSION"))?;
    {
        let store = engine.lock()?;
        let session = &store.session;
        context.counts = Counts {
            tracks: u32::from(session.source.is_some()),
            stems: session
                .studio
                .bank
                .as_ref()
                .map_or(0, |bank| count(bank.stems.tracks.len())),
            eq_bands: count(session.settings.manual_eq.len()),
        };
        context.activity = Activity {
            session_loaded: session.source.is_some()
                || session.studio.origin.is_some()
                || session.studio.bank.is_some(),
            preview_available: session.master.is_some() || session.studio.current.is_some(),
            processing_active: store.active.is_some(),
            // Desktop recovery is not a capability exposed to this connection;
            // no attempt is made to inspect the GUI's private cache.
            recovery_available: false,
        };
    }
    let report = support_report::build(&context, &draft)?;
    let mut reviews = reviews
        .lock()
        .map_err(|_| anyhow::anyhow!("Report review state is unavailable; reconnect"))?;
    let sequence = reviews
        .sequence
        .checked_add(1)
        .context("Report review sequence exhausted; reconnect")?;
    // connection_id is an existing generated cache nonce, not a user name/path.
    // This token binds bytes to a connection, not a human-consent mechanism.
    let id = format!("support-{}-{sequence:016x}", engine.connection_id);
    let result = json!({
        "review_id": id, "preview_markdown": report.markdown(),
        "bytes": report.markdown().len(), "text_was_changed": report.text_was_changed(),
        "review_notice": support_report::REVIEW_NOTICE,
        "client_privacy_notice": CLIENT_PRIVACY_NOTICE,
        "installed_desktop_version": null,
        "scope": "This MCP connection only. No readiness probes or desktop recovery-cache reads. All four tool-readiness values remain unknown (null), not false or a claim of failed installation.",
        "next": "Show the entire preview to the owner. Only after explicit owner review and save approval, call save_support_report with this review_id, a fresh .md filename and owner_review_confirmed:true. A new successful preview invalidates this ID. No report has been saved or submitted."
    });
    reviews.sequence = sequence;
    reviews.current = Some(ReviewedReport { id, report });
    Ok(result)
}

pub(crate) fn save(
    engine: &Engine,
    reviews: &Mutex<ReviewStore>,
    request: SaveSupportReport,
) -> Result<Value> {
    ensure!(
        request.owner_review_confirmed,
        "Owner must review the complete matching preview and explicitly approve saving; owner_review_confirmed defaults false"
    );
    ensure!(
        request.review_id.len() <= 128,
        "Invalid report review ID; prepare and review a report first"
    );
    let filename = file_policy::export_stem(&request.filename)?;
    ensure!(
        filename.to_ascii_lowercase().ends_with(".md"),
        "Choose a fresh .md support-report filename"
    );
    let reviews = reviews
        .lock()
        .map_err(|_| anyhow::anyhow!("Report review state is unavailable; reconnect"))?;
    let reviewed = reviews
        .current
        .as_ref()
        .context("Prepare a support-report preview and obtain owner review first")?;
    ensure!(
        reviewed.id == request.review_id,
        "Report review ID is stale or belongs to another connection; prepare/review the current report"
    );
    // Avoid arbitrary filesystem-error payloads: some wrap the selected path.
    engine.policy.check_output().map_err(|_| {
        anyhow::anyhow!("Authorized output folder is unavailable or changed; reconnect")
    })?;
    reviewed
        .report
        .write_new(&engine.policy.output.join(&filename))?;
    Ok(
        json!({"saved": true, "filename": filename, "bytes": reviewed.report.markdown().len(),
        "review_id": reviewed.id, "uploaded": false,
        "note": "Saved exactly the reviewed preview. Nothing was uploaded or posted. The file may contain user-entered sensitive information; review before sharing."}),
    )
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

pub fn policy() -> Value {
    json!({"prepare_tool":"prepare_support_report", "save_tool":"save_support_report",
        "in_memory_previews_per_connection":1, "max_report_bytes":support_report::MAX_REPORT_BYTES,
        "max_title_utf8_bytes":support_report::MAX_TITLE_BYTES, "max_details_utf8_bytes":support_report::MAX_DETAILS_BYTES,
        "requires_owner_review_before_save":true, "overwrite":false, "network":false,
        "installed_desktop_version":null, "client_privacy_notice":CLIENT_PRIVACY_NOTICE,
        "automatic_diagnostics":"Compiled companion version, linked engine build, OS/architecture, connection counts and yes/no/unknown states only. All four tool-readiness values are unknown (null); no readiness probe is performed. No source/plugin/profile names, raw paths/errors/logs/audio/environment or desktop recovery-cache access.",
        "help":"get_help(topic=support_reports); get_help(topic=recovery) offers desktop guidance only"})
}

pub fn help() -> Value {
    json!({"policy":policy(), "steps":[
        {"tool":"prepare_support_report", "arguments":{"category":"audio_playback", "title":"Playback stops", "details":"1. Import a track\n2. Press Play\n3. Playback stops"}},
        {"action":"Show the ENTIRE returned preview_markdown to the owner, including automatic diagnostics. Explain filtering is best-effort and returned content may go to the AI client's model provider. Obtain explicit review and save approval; do not invent consent."},
        {"tool":"save_support_report", "arguments":{"review_id":"<latest review_id>", "filename":"support-report.md", "owner_review_confirmed":true}}
    ], "failure_recovery":"An invalid prepare or failed save preserves the previous usable report and all audio/project/settings state. A successful new prepare invalidates its predecessor; do not reuse an earlier ID. Existing filenames are refused without suffixing/overwriting; ask for a fresh filename, using the same reviewed ID when still current.",
        "identity":"companion_version is this compiled MCP binary's package version; linked_engine_build is the compiled library, NOT evidence of an installed GUI. installed_desktop_version is always unknown/null. No GUI executable is queried or launched.",
        "security":"For security-sensitive reports use the explicit private advisory route, not a public issue. This companion never opens a browser or submits a report.",
        "private_security_report_url":support_report::PRIVATE_SECURITY_REPORT_URL})
}

pub fn recovery_help() -> Value {
    json!({"scope":"Desktop-only recovery guidance; no MCP recovery-cache filesystem access is granted.",
        "guidance":"Open the desktop app and use its recovery prompt/history if available. Review a restore point there; do not clear or overwrite the current project before reviewing. Missing original audio or failed recovery should preserve current work. MCP does not enumerate, restore or delete private desktop recovery entries.",
        "authorized_access":"Only explicitly authorized input libraries/output root apply to this connection. Never infer permission to read the GUI private cache from a request for help. A restored desktop session and this MCP session are independent.",
        "support":"Use get_help(topic=support_reports) for allowlisted diagnostics; do not paste raw recovery files/logs/errors or project/track names automatically."})
}

#[cfg(test)]
#[path = "support_report_tests.rs"]
mod tests;
