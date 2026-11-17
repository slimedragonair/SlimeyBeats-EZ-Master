use serde_json::{Value, json};
pub fn get() -> Value {
    json!({"topic":"arrangement","workflow":[
        "create_arrangement with explicit project sample_rate/channels; get_arrangement recovers session_id/revision and all stable IDs",
        "load_audio/import_stem_bundle as needed, then add_arrangement_assets with exact owned audio_id values from get_studio_session",
        "edit_arrangement applies one atomic Edit batch and one Undo; it never renders PCM",
        "suggest_arrangement_eq is an optional explicit job; inspect bands/notes/high_frequency then edit_arrangement set_eq only if requested",
        "render_arrangement is an explicit temporary preview job; it does not start playback",
        "export_arrangement is explicit final encoding using the existing native formats; poll get_job_status"],
        "identity":"Names are display labels. Source, lane, clip and effect IDs are explicit; a filename never selects an EQ target. Revision and session ID guards reject stale actions.",
        "units":{"placement":"integer PROJECT frames; end exclusive","clip_source_start":"integer immutable SOURCE frames","mix_lane_eq_range":"PROJECT frames","clip_eq_range":"immutable SOURCE frames (not clip-local or project time)","gain":"dB","pan":"-1 left..1 right"},
        "eq":"scope.kind is mix, lane with lane_id, or clip with clip_id. range:null covers the whole target. Ranges require20ms and smooth5ms inner boundaries. When replacing an existing effect, pass only its exclude_eq_id to suggestions, then set_eq with the same stable effect ID; unrelated effects remain in analysis.",
        "analysis":"Mix uses current faders. Lane/Clip analysis isolates the target and ignores mute/solo; appropriate routed effects remain. The shared target helper allocates only cropped target PCM. Suggestions never apply or master audio.",
        "example_edit":{"session_id":"<current session_id>","expected_revision":1,"edits":[{"kind":"move_clip","clip_id":"<clip ID>","lane_id":"<lane ID>","start_frame":48000}]},
        "history":"undo_arrangement redo:false(default) undoes; redo:true redoes. Sources remain immutable and owned. Clear session releases this workspace too.",
        "limits":"Different source durations and mono-to-stereo are allowed; project sample rates must match without implicit resampling. Existing aligned-bank rules are unchanged. Native import and aggregate decoded/metadata budgets apply.",
        "playback":"This stdio companion exposes temporary rendered previews. Actual local browser/native live editing uses the shared frame player; no realtime playback is claimed through MCP."})
}
