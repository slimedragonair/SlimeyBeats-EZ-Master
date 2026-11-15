//! Shipped guidance for agents that have no source checkout or chat history.
use serde_json::{Value, json};

pub fn get() -> Value {
    json!({
        "purpose":"Find timestamped indicators without changing audio; offer conservative, explicit and reversible repairs. No models, codec setup or optional downloads are needed for already loaded audio.",
        "workflow":[
            {"tool":"get_studio_session","arguments":{},"read":"Choose assets[].audio_id for an original, stem or Studio preview. A source_id returned by load_audio also works."},
            {"tool":"scan_audio_issues","arguments":{"audio_id":"<chosen current audio_id>"},"read":"Poll get_job_status. Read result.report.issues, thresholds, evidence, notes and truncated; no history or audio changed."},
            {"explain":"Discuss the selected finding with the user. Silence and music after a quiet gap may be intentional. Low-mid energy can be a musical choice. A scan cannot prove bleed, clipping history or an instrument's missing harmonics."},
            {"tool":"preview_stem_doctor","arguments":{"audio_id":"<same current audio_id>","repair":{"action":"reduce_low_mids","start_seconds":1.0,"end_seconds":3.0,"strength":0.5}},"read":"Example only: use the actual selected issue's times and supported action, not these invented timestamps. Run only when the user requests a fix. Poll for studio_preview_id and changed_samples. Declip is the other action; silence and possible-ending markers have no automatic repair."},
            {"tool":"scan_audio_issues","arguments":{"audio_id":"<returned studio_preview_id>"},"read":"Optional read-only rescan of the new revision. Invite level-conscious listening; do not claim to have heard it."},
            {"tool":"undo_studio_edit","arguments":{},"read":"Use if requested. Poll, then refresh current IDs with get_studio_session. Redo is available while retained."}
        ],
        "repair_fields":{"action":"declip or reduce_low_mids; choose only an appropriate explicit repair","start_seconds":"Finite numeric seconds >=0, selecting at least one frame","end_seconds":"Finite numeric seconds >start, <=duration; selected end is exclusive after nearest-frame rounding","strength":"Finite 0..1; 0 bypass, 0.5 gentle starting point, 1 strongest bounded setting"},
        "direct_stem":"A repair of a stem audio_id prepares that stem alone. Before chaining another operation, apply_preview_to_stem with its current stem_set_id, returned studio_preview_id and stable kind to update the immutable bank and whole mix. This returns new bank/asset IDs. Scans and unapplied previews do not change the bank.",
        "limits":"These deterministic indicators are not a listening judgment or calibrated probability. Thresholds and measured evidence are reported. Low-mid reduction is gentle EQ, not an objective mud cure; declipping only estimates eligible short full-scale runs. Duration, rate, channels and raw samples outside the selected region are retained. No automatic silence removal, arrangement changes, stem isolation or missing-harmonic regeneration. A truncated report is incomplete.",
        "save":"The temporary preview is separate from final export and mastering. export_studio_audio/export_stems/save_studio_project require their own user request. Audition in desktop only when requested/configured; desktop and AI revisions evolve independently."
    })
}
