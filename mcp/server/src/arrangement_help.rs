use serde_json::{Value, json};
pub fn get() -> Value {
    let mut help = json!({"topic":"arrangement","workflow":[
        "create_arrangement with explicit project sample_rate/channels; get_arrangement recovers session_id/revision and all stable IDs",
        "For each ordinary source: load_audio, poll get_job_status until completed, get_studio_session, then add_arrangement_assets with that exact owned audio_id BEFORE loading the next file; repeat this loop for every source",
        "edit_arrangement applies one atomic Edit batch and one Undo; it never renders PCM",
        "suggest_arrangement_eq is an optional explicit job; inspect bands/notes/high_frequency then edit_arrangement set_eq only if requested",
        "render_arrangement is an explicit temporary preview job; it does not start playback",
        "export_arrangement is explicit final encoding using the existing native formats; poll get_job_status"],
        "identity":"Names are display labels. Source, lane, clip and effect IDs are explicit; a filename never selects an EQ target. Revision and session ID guards reject stale actions.",
        "source_ownership":"The next load_audio replaces the current Studio selection. Assets already attached with add_arrangement_assets remain owned by the arrangement, including their immutable PCM. Load one, attach it, then repeat. Use the newest arrangement revision for each attachment/edit; get_arrangement recovers it. import_stem_bundle retains its separate aligned-bank requirements.",
        "units":{"placement":"integer PROJECT frames; end exclusive","clip_source_start":"integer immutable SOURCE frames","mix_lane_eq_range":"PROJECT frames","clip_eq_range":"immutable SOURCE frames (not clip-local or project time)","gain":"dB","pan":"-1 left..1 right"},
        "eq":"scope.kind is mix, lane with lane_id, or clip with clip_id. range:null covers the whole target. Ranges require20ms and smooth5ms inner boundaries. When replacing an existing effect, pass only its exclude_eq_id to suggestions with the EXACT same scope and range, then set_eq with the same stable effect ID; unrelated effects remain in analysis. Arrangement EQ requires no master_preview or apply_eq.",
        "analysis":"Mix uses current faders. Lane/Clip analysis isolates the target and ignores mute/solo; appropriate routed effects remain. The shared target helper allocates only cropped target PCM. Suggestions never apply or master audio.",
        "example_edit":{"session_id":"<current session_id>","expected_revision":1,"edits":[{"kind":"move_clip","clip_id":"<clip ID>","lane_id":"<lane ID>","start_frame":48000}]},
        "example_set_eq":{
            "context":"Illustrative requested cut, not a recommendation: 48kHz project, Voice clip starts at PROJECT frame96000 (song2s), source_start_frame0. Song3–4s maps to clip SOURCE frames48000–96000. Lane/mix for the same song interval instead use PROJECT frames144000–192000. Resolve Voice to its explicit clip ID first.",
            "tool":"edit_arrangement","arguments":{"session_id":"<current session_id>","expected_revision":2,"edits":[{
                "kind":"set_eq","effect":{"id":"voice-high-section","scope":{"kind":"clip","clip_id":"<explicit Voice clip ID>"},
                "range":{"start_frame":48000,"end_frame":96000},"output_gain_db":0.0,"bypass":false,"bands":[
                    {"type":"low_shelf","freq":50.0,"gain":0.0,"q":0.707,"on":true},
                    {"type":"bell","freq":120.0,"gain":0.0,"q":0.707,"on":true},
                    {"type":"bell","freq":300.0,"gain":0.0,"q":0.707,"on":true},
                    {"type":"bell","freq":800.0,"gain":0.0,"q":0.707,"on":true},
                    {"type":"bell","freq":2000.0,"gain":0.0,"q":0.707,"on":true},
                    {"type":"bell","freq":5000.0,"gain":0.0,"q":0.707,"on":true},
                    {"type":"high_shelf","freq":10000.0,"gain":-6.0,"q":0.707,"on":true}]}}]},
            "after":"Use the returned revision for render_arrangement and poll its job. Undo/Redo requires the latest revision; render again after either because the old preview is stale. Export is a separate explicitly requested action."},
        "history":"undo_arrangement redo:false(default) undoes; redo:true redoes. Sources remain immutable and owned. Clear session releases this workspace too.",
        "limits":"Different source durations and mono-to-stereo are allowed; project sample rates must match without implicit resampling. Existing aligned-bank rules are unchanged. Native import and aggregate decoded/metadata budgets apply.",
        "playback":"This stdio companion exposes temporary rendered previews. Actual local browser/native live editing uses the shared frame player; no realtime playback is claimed through MCP."});
    help["persistence"] = crate::arrangement_project_help::get();
    help
}
