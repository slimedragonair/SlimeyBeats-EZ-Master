import test from "node:test";
import assert from "node:assert/strict";
import { ArrangementTimelineState, secondsToFrame, frameSeconds } from "./arrangement-timeline-state.js";

function project() {
    const clip = (id, start, source, duration, sourceId) => ({ id, source_id: sourceId, start_frame: start,
        source_start_frame: source, end_frame: start + duration, gain_db: 0, fade_in_frames: 0, fade_out_frames: 0 });
    return { version: 1, sample_rate: 48000, channels: 2, eq: [], lanes: [
        { id: "lane-a", name: "Drums", gain_db: 0, pan: 0, mute: false, solo: false,
            clips: [clip("clip-a", 48000, 24000, 96000, "asset-a")] },
        { id: "lane-b", name: "Drums", gain_db: 0, pan: 0, mute: false, solo: false,
            clips: [clip("clip-b", 0, 0, 24000, "asset-b")] },
    ] };
}

test("different-duration clips and repeated labels retain stable selection through metadata revisions", () => {
    const state = new ArrangementTimelineState(), initial = project();
    state.configure(initial, 1);
    state.selectClip("clip-a");
    state.setRange(48000, 96000);
    state.configure(structuredClone(initial), 2);
    assert.equal(state.selectedClip, "clip-a");
    assert.equal(state.selectedLane, "lane-a");
    assert.equal(state.frames, 144000);
    assert.deepEqual([state.start, state.end], [48000, 96000]);
    const next = project(); next.lanes[0].clips = [];
    state.configure(next, 3);
    assert.equal(state.selectedClip, null);
    assert.equal(state.selectedLane, "lane-a");
});

test("a move/trim gesture never mutates source metadata and uses separate project/source coordinates", () => {
    const state = new ArrangementTimelineState(), initial = project(), before = structuredClone(initial);
    state.configure(initial, 1);
    const moved = state.movePreview("clip-a", 0, "lane-b");
    assert.equal(moved.clip.start_frame, 0);
    assert.equal(moved.clip.end_frame, 96000);
    assert.equal(moved.clip.source_start_frame, 24000);
    assert.equal(moved.lane_id, "lane-b");
    const start = state.trimPreview("clip-a", "start", 0, 240000);
    assert.equal(start.clip.start_frame, 24000);
    assert.equal(start.clip.source_start_frame, 0);
    const end = state.trimPreview("clip-a", "end", 999999, 144000);
    assert.equal(end.clip.end_frame, 168000);
    assert.deepEqual(initial, before);
});

test("scope and exact end-exclusive range stay explicit; missing selection never becomes whole-mix EQ", () => {
    const state = new ArrangementTimelineState(); state.configure(project(), 1);
    state.scope = "lane";
    assert.ok(state.eqTarget().error);
    state.selectClip("clip-a");
    assert.deepEqual(state.eqTarget().scope, { kind: "lane", lane_id: "lane-a" });
    state.scope = "clip";
    assert.deepEqual(state.eqTarget().scope, { kind: "clip", clip_id: "clip-a" });
    state.scope = "range";
    assert.ok(state.eqTarget().error);
    state.setRange(48001, 96000);
    assert.deepEqual(state.eqTarget().range, { start_frame: 48001, end_frame: 96000 });
    state.rangeTarget = "mix";
    assert.deepEqual(state.eqTarget().scope, { kind: "mix" });
    state.rangeTarget = "clip";
    assert.deepEqual(state.eqTarget().range, { start_frame: 24001, end_frame: 72000 });
    assert.match(state.eqTarget().label, /song .*source/);
    state.setRange(48000, 48001);
    assert.match(state.eqTarget().error, /20 ms/);
    state.scope = "clip";
    assert.ok(state.eqTarget().scope);
    state.scope = "range";
    state.setRange(0, 48000);
    assert.ok(state.eqTarget().error);
    state.scope = "clip";
    state.selectLane("lane-b");
    assert.ok(state.eqTarget().error);
});

test("precise typed times preserve single-frame bounds; blank, partial and nonfinite drafts reject", () => {
    for (const rate of [32000, 44100, 48000, 96000]) for (const frame of [0, 1, rate - 1, rate * 5 + 1])
        assert.equal(secondsToFrame(frameSeconds(frame, rate), rate), frame);
    for (const text of ["", " ", "-", "1e", "NaN", "Infinity", "-1"])
        assert.throws(() => secondsToFrame(text, 48000));
    const state = new ArrangementTimelineState(); state.configure(project(), 1);
    state.selectRange(12, 12);
    assert.deepEqual([state.start, state.end], [12, 13]);
    assert.throws(() => state.setRange(12, 12));
    assert.throws(() => state.movePreview("clip-a", Number.MAX_SAFE_INTEGER, "lane-b"));
});

test("fit selection keeps a single frame visible at the minimum 10 ms view", () => {
    const state = new ArrangementTimelineState(); state.configure(project(), 1);
    assert.equal(state.fitSelection(), false);
    state.selectClip("clip-a");
    assert.equal(state.fitSelection(), true);
    assert.deepEqual([state.left, state.right], [48000, 144000]);
    state.setRange(144000 - 1, 144000);
    assert.equal(state.fitSelection(), true);
    assert.equal(state.span, 480);
    assert.ok(state.left <= state.start && state.right >= state.end);
    state.setRange(0, 1); state.fitSelection();
    assert.deepEqual([state.left, state.right], [0, 480]);
});

test("first import fits all parts while later revisions retain the chosen zoom", () => {
    const state = new ArrangementTimelineState();
    state.configure({ version: 1, sample_rate: 48000, channels: 2, lanes: [], eq: [] }, 0);
    assert.equal(state.right, 48000);
    const assets = [{ id: "asset-a", name: "A" }, { id: "asset-b", name: "B" }];
    state.configure(project(), 1, assets);
    assert.deepEqual([state.left, state.right], [0, 144000]);
    state.zoom(0.5);
    const view = [state.left, state.right];
    state.configure(project(), 2, assets);
    assert.deepEqual([state.left, state.right], view);
    state.configure({ version: 1, sample_rate: 48000, channels: 2, lanes: [], eq: [] }, 3, assets);
    state.configure(project(), 4, assets);
    assert.deepEqual([state.left, state.right], view);
});
