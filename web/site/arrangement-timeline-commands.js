import { secondsToFrame, frameNumber, clipDuration } from "./arrangement-timeline-state.js";

export function finiteDraft(text, label, min, max) {
    if (typeof text !== "string" || !text.trim()) throw new Error(`Enter ${label.toLowerCase()}.`);
    const value = Number(text);
    if (!Number.isFinite(value) || value < min || value > max) throw new Error(`${label} must be ${min} to ${max}.`);
    return value;
}

export function timingEdits(model, draft) {
    const entry = model.clip();
    if (!entry) throw new Error("Select a clip first.");
    const start = secondsToFrame(draft.start, model.rate), end = secondsToFrame(draft.end, model.rate);
    const source = secondsToFrame(draft.source, model.rate);
    if (end <= start) throw new Error("Clip end must be after its start.");
    if (!model.lane(draft.lane)) throw new Error("Choose an available destination track.");
    const trim = { kind: "trim_clip", clip_id: entry.clip.id, start_frame: start, source_start_frame: source, end_frame: end };
    if (draft.lane === entry.lane.id) return [trim];
    return [trim, { kind: "move_clip", clip_id: entry.clip.id, lane_id: draft.lane, start_frame: start }];
}

export function clipLevelEdits(model, draft) {
    const clip = model.clip()?.clip;
    if (!clip) throw new Error("Select a clip first.");
    const first = secondsToFrame(draft.fadeIn, model.rate), last = secondsToFrame(draft.fadeOut, model.rate);
    if (first > clipDuration(clip) || last > clipDuration(clip)) throw new Error("Keep each fade within the clip duration.");
    return [
        { kind: "set_clip_name", clip_id: clip.id, name: draft.name.trim() || null },
        { kind: "set_clip_gain", clip_id: clip.id, gain_db: finiteDraft(draft.gain, "Clip gain", -60, 12) },
        { kind: "set_clip_fades", clip_id: clip.id, fade_in_frames: first, fade_out_frames: last },
    ];
}

export function laneEdit(model, draft) {
    const lane = model.lane();
    if (!lane) throw new Error("Select a track first.");
    const name = draft.name.trim();
    if (!name) throw new Error("Enter a track name.");
    const pan = finiteDraft(draft.pan, "Balance", -1, 1);
    if (model.arrangement.channels === 1 && pan !== 0) throw new Error("Balance requires a stereo project.");
    return { kind: "set_lane", lane_id: lane.id, name, gain_db: finiteDraft(draft.gain, "Track gain", -60, 12),
        pan, mute: !!draft.mute, solo: !!draft.solo };
}

export function duplicateEdit(model, newId) {
    const entry = model.clip();
    if (!entry) throw new Error("Select a clip to duplicate.");
    frameNumber(entry.clip.end_frame + clipDuration(entry.clip));
    return { kind: "duplicate_clip", clip_id: entry.clip.id, new_clip_id: newId,
        lane_id: entry.lane.id, start_frame: entry.clip.end_frame };
}
