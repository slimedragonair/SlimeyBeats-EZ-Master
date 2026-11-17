import { ArrangementDrafts } from "./arrangement-timeline-drafts.js";
import { frameSeconds, secondsToFrame, frameNumber, trackLabel } from "./arrangement-timeline-state.js";
import { timingEdits, clipLevelEdits, laneEdit, duplicateEdit } from "./arrangement-timeline-commands.js";
import { setOptions as options, setInputValue, setText } from "./arrangement-timeline-fields.js";

const CLIP_FIELDS = ["clip-name", "clip-lane", "clip-start", "clip-source", "clip-end", "clip-gain", "fade-in", "fade-out"];
const TIMING_FIELDS = ["clip-lane", "clip-start", "clip-source", "clip-end"];
const LEVEL_FIELDS = ["clip-name", "clip-gain", "fade-in", "fade-out"];
const LANE_FIELDS = ["lane-name", "lane-gain", "lane-pan", "lane-mute", "lane-solo"];
const FIELD_PROPERTIES = { "clip-name": "name", "clip-gain": "gain_db", "fade-in": "fade_in_frames", "fade-out": "fade_out_frames",
    "lane-name": "name", "lane-gain": "gain_db", "lane-pan": "pan", "lane-mute": "mute", "lane-solo": "solo" };
const CONFLICT_MESSAGE = "The saved item changed. Your unfinished values are kept; reload saved values to resolve them.";

export function wireArrangementInspector(host, { model, getState, command, position, repaint, notice }) {
    const $ = name => host.querySelector(`[data-arr="${name}"]`), drafts = new ArrangementDrafts();
    const focused = new Map();
    let disposed = false;
    function entity(group) {
        const state = model();
        if (group !== "lane") {
            const entry = state.clip();
            return { key: entry ? `${group}:${entry.clip.id}` : null, record: entry ? { lane_id: entry.lane.id,
                duration_frames: entry.clip.end_frame - entry.clip.start_frame, ...entry.clip, name: entry.clip.name ?? null } : null };
        }
        const lane = state.lane();
        return { key: lane ? `lane:${lane.id}` : null, record: lane ? { id: lane.id, name: lane.name,
            gain_db: lane.gain_db, pan: lane.pan, mute: lane.mute, solo: lane.solo, channels: state.arrangement.channels } : null };
    }
    function field(name, key, record, value) {
        const element = $(name), saved = drafts.value(key, name, value);
        setInputValue(element, saved);
    }
    function raw(name) { return $(name).type === "checkbox" ? $(name).checked : $(name).value; }
    for (const [group, fields] of [["timing", TIMING_FIELDS], ["level", LEVEL_FIELDS], ["lane", LANE_FIELDS]]) for (const name of fields) {
        const watched = group === "timing" ? ["id", "source_id", "lane_id", "start_frame", "source_start_frame", "end_frame"]
            : ["id", ...(group === "level" ? ["source_id"] : []), FIELD_PROPERTIES[name],
                ...(["fade-in", "fade-out"].includes(name) ? ["source_start_frame", "duration_frames"] : []),
                ...(name === "lane-pan" ? ["channels"] : [])];
        $(name).addEventListener("focus", () => { if (!disposed) focused.set(name, entity(group)); });
        $(name).addEventListener("input", () => {
            if (disposed) return;
            const item = entity(group), base = focused.get(name);
            drafts.set(item.key, base?.key === item.key ? base.record : item.record, name, raw(name), watched);
            setText($("clip-error"), "");
            sync();
        });
        $(name).addEventListener("blur", () => {
            if (disposed) return;
            const base = focused.get(name), current = entity(group); focused.delete(name);
            if (base?.key && base.key === current.key && watched.some(property => JSON.stringify(base.record?.[property]) !== JSON.stringify(current.record?.[property])))
                drafts.set(current.key, base.record, name, raw(name), watched);
            sync();
        });
    }
    $("lane-select").onchange = () => { model().selectLane($("lane-select").value); repaint(true); };
    $("clip-select").onchange = () => { model().selectClip($("clip-select").value); repaint(true); };
    async function dispatch(edits, key) {
        const accepted = await command(edits);
        if (accepted && key) drafts.clear(key);
        sync();
        return accepted;
    }
    async function act(action) {
        if (disposed || getState().busy) return;
        const state = model(), selected = state.clip(), lane = state.lane();
        const timing = entity("timing"), level = entity("level"), laneEntity = entity("lane");
        try {
            if (action === "clip-reload" || action === "lane-reload") {
                if (action === "clip-reload") { drafts.clear(timing.key); drafts.clear(level.key); }
                else drafts.clear(laneEntity.key);
                setText($("clip-error"), "");
                sync(); return;
            }
            if (action === "clip-timing") {
                drafts.requireCurrent(timing.key, timing.record);
                return await dispatch(timingEdits(state, { start: raw("clip-start"), end: raw("clip-end"),
                    source: raw("clip-source"), lane: raw("clip-lane") }), timing.key);
            }
            if (action === "clip-level") {
                drafts.requireCurrent(level.key, level.record);
                return await dispatch(clipLevelEdits(state, { name: raw("clip-name"), gain: raw("clip-gain"),
                    fadeIn: raw("fade-in"), fadeOut: raw("fade-out") }), level.key);
            }
            if (action === "lane-update") {
                drafts.requireCurrent(laneEntity.key, laneEntity.record);
                return await dispatch(laneEdit(state, { name: raw("lane-name"), gain: raw("lane-gain"),
                    pan: raw("lane-pan"), mute: raw("lane-mute"), solo: raw("lane-solo") }), laneEntity.key);
            }
            if (["lane-up", "lane-down", "delete-lane"].includes(action)) {
                if (!lane) throw new Error("Select a track first.");
                if (action === "delete-lane") return await dispatch({ kind: "remove_lane", lane_id: lane.id });
                const index = state.lanes.findIndex(entry => entry.id === lane.id) + (action === "lane-up" ? -1 : 1);
                if (index >= 0 && index < state.lanes.length) return await dispatch({ kind: "reorder_lane", lane_id: lane.id, index });
                return;
            }
            if (!selected) throw new Error("Select a clip first.");
            if (action === "duplicate") {
                const next = duplicateEdit(state, crypto.randomUUID());
                if (await dispatch(next)) { state.selectClip(next.new_clip_id); repaint(true); }
                return;
            }
            if (action === "delete-clip") return await dispatch({ kind: "delete_clip", clip_id: selected.clip.id });
            if (action === "split") {
                const at = frameNumber(Math.round(position()));
                if (at <= selected.clip.start_frame || at >= selected.clip.end_frame)
                    throw new Error("Place the playhead strictly inside the selected clip before splitting.");
                const id = crypto.randomUUID();
                if (await dispatch({ kind: "split_clip", clip_id: selected.clip.id, at_frame: at, new_clip_id: id })) {
                    state.selectClip(id); repaint(true);
                }
                return;
            }
            if (action === "nudge-left" || action === "nudge-right") return await nudge(action === "nudge-left" ? -1 : 1);
        } catch (error) {
            setText($("clip-error"), error.message || String(error)); notice(error.message || String(error), true);
        }
    }
    async function nudge(direction, multiplier = 1) {
        try {
            const state = model(), selected = state.clip();
            if (!selected || getState().busy) return;
            const step = secondsToFrame(raw("nudge"), state.rate) * multiplier;
            if (step < 1) throw new Error("Choose a nudge of at least one sample frame.");
            const first = Math.max(0, selected.clip.start_frame + direction * step);
            if (first === selected.clip.start_frame) return;
            const preview = state.movePreview(selected.clip.id, first, selected.lane.id);
            return await dispatch({ kind: "move_clip", clip_id: selected.clip.id, lane_id: selected.lane.id, start_frame: preview.clip.start_frame });
        } catch (error) { notice(error.message || String(error), true); }
    }
    const actions = ["clip-timing", "clip-level", "clip-reload", "lane-update", "lane-reload", "lane-up", "lane-down",
        "delete-lane", "split", "duplicate", "delete-clip", "nudge-left", "nudge-right"];
    for (const action of actions) $(action).onclick = () => act(action);
    function sync() {
        if (disposed) return;
        const state = model(), selected = state.clip(), lane = state.lane(), { busy, assets = [] } = getState();
        const assetById = new Map(assets.map(asset => [asset.id, asset]));
        options($("lane-select"), [["", "Choose a track"], ...state.lanes.map((lane, index) => [lane.id, trackLabel(lane, index)])], state.selectedLane);
        options($("clip-select"), [["", "Choose a clip"], ...(lane?.clips || []).map((clip, index) => [clip.id,
            `${index + 1} · ${clip.name || assetById.get(clip.source_id)?.name || "Audio clip"} · ${frameSeconds(clip.start_frame, state.rate)} s`])], state.selectedClip);
        options($("clip-lane"), state.lanes.map((lane, index) => [lane.id, trackLabel(lane, index)]), drafts.value(entity("timing").key, "clip-lane", selected?.lane.id));
        const timing = entity("timing"), level = entity("level"), laneEntity = entity("lane");
        const clip = selected?.clip;
        const values = { "clip-name": clip?.name || "", "clip-lane": selected?.lane.id || "", "clip-start": frameSeconds(clip?.start_frame || 0, state.rate),
            "clip-source": frameSeconds(clip?.source_start_frame || 0, state.rate), "clip-end": frameSeconds(clip?.end_frame || 0, state.rate),
            "clip-gain": clip?.gain_db || 0, "fade-in": frameSeconds(clip?.fade_in_frames || 0, state.rate), "fade-out": frameSeconds(clip?.fade_out_frames || 0, state.rate) };
        for (const name of CLIP_FIELDS) {
            const item = TIMING_FIELDS.includes(name) ? timing : level;
            field(name, item.key, item.record, values[name]); $(name).disabled = busy || !clip;
        }
        const laneValues = { "lane-name": lane?.name || "", "lane-gain": lane?.gain_db || 0, "lane-pan": lane?.pan || 0, "lane-mute": lane?.mute, "lane-solo": lane?.solo };
        for (const name of LANE_FIELDS) { field(name, laneEntity.key, laneEntity.record, laneValues[name]); $(name).disabled = busy || !lane; }
        $("lane-pan").disabled ||= state.arrangement?.channels === 1;
        $("clip-label").textContent = clip ? `${clip.name || assetById.get(clip.source_id)?.name || "Audio clip"} · track ${state.lanes.indexOf(selected.lane) + 1}, clip ${selected.lane.clips.indexOf(clip) + 1}` : "Select a clip.";
        for (const action of actions) $(action).disabled = busy || (action.startsWith("lane-") || action === "delete-lane" ? !lane : !clip);
        $("lane-up").disabled ||= state.lanes[0]?.id === lane?.id;
        $("lane-down").disabled ||= state.lanes.at(-1)?.id === lane?.id;
        $("clip-reload").disabled ||= !drafts.changed(timing.key) && !drafts.changed(level.key);
        $("lane-reload").disabled ||= !drafts.changed(laneEntity.key);
        if (drafts.conflicts(timing.key, timing.record) || drafts.conflicts(level.key, level.record) || drafts.conflicts(laneEntity.key, laneEntity.record))
            setText($("clip-error"), CONFLICT_MESSAGE);
        else if ($("clip-error").textContent === CONFLICT_MESSAGE) setText($("clip-error"), "");
    }
    return { sync, action: act, nudge, reset() { drafts.reset(); sync(); }, dispose() { disposed = true; drafts.reset(); focused.clear(); } };
}
