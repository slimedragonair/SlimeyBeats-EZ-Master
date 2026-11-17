import test from "node:test";
import assert from "node:assert/strict";
import { ArrangementTimelineState } from "./arrangement-timeline-state.js";
import { timingEdits, clipLevelEdits, laneEdit, duplicateEdit, finiteDraft } from "./arrangement-timeline-commands.js";
import { ArrangementDrafts } from "./arrangement-timeline-drafts.js";

function model() {
    const state = new ArrangementTimelineState();
    state.configure({ version: 1, sample_rate: 48000, channels: 2, eq: [], lanes: [
        { id: "a", name: "Track A", gain_db: 0, pan: 0, mute: false, solo: false, clips: [
            { id: "clip", source_id: "immutable", start_frame: 48000, source_start_frame: 24000,
                end_frame: 144000, gain_db: 0, fade_in_frames: 0, fade_out_frames: 0 },
        ] },
        { id: "b", name: "Track B", gain_db: 0, pan: 0, mute: false, solo: false, clips: [] },
    ] }, 4);
    state.selectClip("clip");
    return state;
}

test("precise timing emits the real core trim and cross-lane move payloads as one batch", () => {
    const state = model(), before = structuredClone(state.arrangement);
    assert.deepEqual(timingEdits(state, { start: "2", end: "3", source: "0.5", lane: "b" }), [
        { kind: "trim_clip", clip_id: "clip", start_frame: 96000, source_start_frame: 24000, end_frame: 144000 },
        { kind: "move_clip", clip_id: "clip", lane_id: "b", start_frame: 96000 },
    ]);
    assert.deepEqual(state.arrangement, before);
    for (const [start, end] of [["", "3"], ["2", "2"], ["NaN", "3"]])
        assert.throws(() => timingEdits(state, { start, end, source: "0", lane: "b" }));
});

test("clip names/gain/fades preserve IDs and reject invalid drafts before dispatch", () => {
    const state = model();
    assert.deepEqual(clipLevelEdits(state, { name: "", gain: "-2.5", fadeIn: "0.1", fadeOut: "0.2" }), [
        { kind: "set_clip_name", clip_id: "clip", name: null },
        { kind: "set_clip_gain", clip_id: "clip", gain_db: -2.5 },
        { kind: "set_clip_fades", clip_id: "clip", fade_in_frames: 4800, fade_out_frames: 9600 },
    ]);
    assert.throws(() => clipLevelEdits(state, { name: "", gain: "", fadeIn: "0", fadeOut: "0" }));
    assert.throws(() => clipLevelEdits(state, { name: "", gain: "0", fadeIn: "3", fadeOut: "0" }));
    assert.throws(() => finiteDraft("Infinity", "Gain", -60, 12));
    assert.throws(() => finiteDraft("12.25", "Gain", -60, 12));
    assert.deepEqual(duplicateEdit(state, "new-clip"), { kind: "duplicate_clip", clip_id: "clip",
        new_clip_id: "new-clip", lane_id: "a", start_frame: 144000 });
});

test("track commands remain explicit and cannot imply a pan/downmix on a mono project", () => {
    const state = model(), draft = { name: "Layer", gain: "1", pan: "0.25", mute: false, solo: true };
    assert.deepEqual(laneEdit(state, draft), { kind: "set_lane", lane_id: "a", name: "Layer", gain_db: 1,
        pan: 0.25, mute: false, solo: true });
    state.arrangement.channels = 1;
    assert.throws(() => laneEdit(state, draft), /stereo/);
});

test("raw drafts follow stable IDs, retain invalid text, and require review after conflicting metadata", () => {
    const drafts = new ArrangementDrafts(), before = { source_start_frame: 24000 };
    drafts.set("clip:a", before, "start", "");
    drafts.set("clip:a", before, "end", "1e");
    assert.equal(drafts.value("clip:a", "start", "0"), "");
    assert.equal(drafts.value("clip:a", "end", "0"), "1e");
    assert.equal(drafts.value("clip:b", "end", "3"), "3");
    drafts.requireCurrent("clip:a", structuredClone(before));
    assert.throws(() => drafts.requireCurrent("clip:a", { source_start_frame: 48000 }), /draft is kept/);
    drafts.clear("clip:a");
    assert.equal(drafts.changed("clip:a"), false);
});

test("unrelated accepted controls do not invalidate a draft or erase another operation's values", () => {
    const drafts = new ArrangementDrafts(), before = { id: "lane", name: "Drums", mute: false };
    drafts.set("lane", before, "name", "New label", ["id", "name"]);
    drafts.requireCurrent("lane", { ...before, mute: true });
    assert.equal(drafts.value("lane", "name", "Drums"), "New label");
    assert.throws(() => drafts.requireCurrent("lane", { ...before, name: "Other label" }));
    drafts.set("timing:clip", { start_frame: 48000 }, "start", "1.125");
    drafts.set("level:clip", { gain_db: 0 }, "gain", "-2");
    drafts.clear("level:clip");
    assert.equal(drafts.value("timing:clip", "start", "1"), "1.125");
});
