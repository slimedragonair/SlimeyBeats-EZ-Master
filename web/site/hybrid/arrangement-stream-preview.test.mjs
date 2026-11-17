import test from "node:test";
import assert from "node:assert/strict";
import { arrangementStreamFrame, checkedArrangementStream } from "./arrangement-stream-contract.js";
import { createArrangementStreamPreview } from "./arrangement-stream-preview.js";

const snapshot = (rate = 44100, revision = 0, frames = rate * 8) => ({ epoch: "epoch", revision,
    arrangement: { sample_rate: rate, channels: 2 }, preview: { identity: `epoch:${revision}:after`, frames, prepared_revision: revision, pending: false } });
const descriptor = (value, side = "after", start = 0, id = "stream") => ({ stream_id: id, stream_path: `/v2/projects/project/arrangement/streams/${id}`,
    epoch: value.epoch, prepared_revision: value.revision, identity: value.preview.identity, sample_rate: value.arrangement.sample_rate,
    channels: 2, duration_frames: value.preview.frames, side_duration_frames: side === "before" ? value.arrangement.sample_rate * 4 : value.preview.frames,
    start_frame: start, preview_sample_rate: 48000, preview_priming_frames: 1152 });

function harness(value = snapshot()) {
    const calls = [], stops = []; let latest = value, delay, notify = () => {};
    const player = { playing: false, seconds: 0, plays: 0, volume: .7, position() { return this.seconds; },
        pause() { this.playing = false; notify(); }, clear() { this.pause(); }, releaseExcept() {}, updateGains() {},
        setOriginal(source) { this.source = source; this.seconds = 0; }, async play() { this.playing = true; this.plays++; notify(); } };
    const preview = createArrangementStreamPreview({ createPlayer: changed => { notify = changed; return player; }, mediaUrl: value => value.stream_path,
        stop: async value => stops.push(value.stream_id), prepare: async request => {
            calls.push(request); const current = latest;
            if (delay) await delay;
            const result = descriptor(current, request.side, request.start_frame, `stream_${calls.length}`);
            result.start_frame = Math.min(request.start_frame, result.side_duration_frames); return result;
        } });
    return { ...preview, player, calls, stops, latest: value => latest = value,
        hold() { let release; delay = new Promise(resolve => release = resolve); return () => { delay = null; release(); }; } };
}

test("compressed-preview time converts explicitly to 8k, 44.1k and 48k project frames", () => {
    for (const rate of [8000, 44100, 48000]) {
        const value = snapshot(rate), stream = descriptor(value, "after", rate);
        assert.equal(arrangementStreamFrame(stream, .024), rate);
        assert.equal(arrangementStreamFrame(stream, 1.024), rate * 2);
        assert.equal(arrangementStreamFrame(stream, 100), rate * 8);
    }
});
test("stream admission rejects stale revision, layout mismatch and missing priming instead of guessing", () => {
    const value = snapshot(), stream = descriptor(value);
    assert.equal(checkedArrangementStream(stream, value, "after").side, "after");
    for (const update of [{ prepared_revision: 9 }, { epoch: "other" }, { sample_rate: 48000 }, { side_duration_frames: 0 },
        { preview_priming_frames: undefined }, { stream_id: "../secret" }, { start_frame: 999999 }])
        assert.throws(() => checkedArrangementStream({ ...stream, ...update }, value, "after"));
});
test("a Stop during server preparation cannot start late audio or reset the stopped frame", async () => {
    const h = harness(); await h.adopt(snapshot()); const release = h.hold();
    const start = h.facade.play(); await Promise.resolve(); await Promise.resolve(); h.facade.stop(); release(); await start;
    assert.equal(h.player.plays, 0); assert.equal(h.facade.playing, false); assert.equal(h.facade.positionFrame(), 0);
});

test("Play immediately after a pending seek retains the exact requested frame", async () => {
    const h = harness(); await h.adopt(snapshot()); const release = h.hold();
    const seek = h.facade.seekFrame(88200);
    assert.equal(h.facade.positionFrame(), 88200); assert.equal(h.facade.pending, true);
    const play = h.facade.play(); await Promise.resolve(); await Promise.resolve(); release();
    await Promise.all([seek, play]);
    assert.equal(h.calls.at(-1).start_frame, 88200); assert.equal(h.facade.playing, true); assert.equal(h.facade.pending, false);
});

test("an internal media interruption retires the stream and preserves the stopped project frame", async () => {
    const h = harness(); await h.adopt(snapshot()); await h.facade.play();
    h.player.seconds = 1.024; h.player.pause(); await Promise.resolve(); await Promise.resolve();
    assert.equal(h.facade.playing, false); assert.equal(h.facade.positionFrame(), 44100);
    assert.equal(h.stops.length > 0, true); assert.equal(h.facade.pending, false);
});
test("running A/B retains position and paused A/B remains paused", async () => {
    const h = harness(), value = snapshot(); await h.adopt(value); await h.facade.play();
    h.player.seconds = 1.024; await h.facade.choose("before");
    assert.equal(h.facade.playing, true); assert.equal(h.facade.positionFrame(), 44100);
    h.facade.pause(); const count = h.player.plays; await h.facade.choose("after");
    assert.equal(h.player.plays, count); assert.equal(h.facade.playing, false); assert.equal(h.facade.positionFrame(), 44100);
});
test("fast prepared revision keeps the active stream while a structural change prepares at its audible frame", async () => {
    const h = harness(), first = snapshot(); await h.adopt(first); await h.facade.play(); h.player.seconds = .524;
    const before = h.calls.length, next = snapshot(44100, 1); h.latest(next); await h.adopt(next);
    assert.equal(h.calls.length, before); assert.equal(h.facade.identity, "epoch:1:after"); assert.equal(h.facade.playing, true);
    const structural = snapshot(44100, 2, 44100 * 10); structural.preview.pending = true; structural.preview.prepared_revision = 1;
    h.latest(structural); await h.adopt(structural);
    assert.equal(h.calls.length, before + 1); assert.equal(h.calls.at(-1).start_frame, 22050); assert.equal(h.facade.playing, true);
});
