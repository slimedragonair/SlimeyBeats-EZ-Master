// Run from source: node --test web/tools/timeline_state.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { TimelineState, gainError, frameText, rulerTicks } from "../site/studio-timeline-state.js";
import { createEnvelopeCache } from "../site/studio-timeline-envelope.js";

function state(rate = 48000, length = 48000) {
    const value = new TimelineState();
    value.configure({ sampleRate: rate, length });
    return value;
}
function buffer(channels, rate = 48000) {
    let reads = 0;
    return { sampleRate: rate, length: channels[0].length, numberOfChannels: channels.length,
        getChannelData(index) { reads++; return channels[index]; }, get reads() { return reads; } };
}

test("one-frame edits at EOF remain separate from the ten-millisecond view minimum", () => {
    for (const rate of [8000, 44100, 48000, 96000, 384000]) {
        const value = state(rate);
        assert.equal(value.setSeconds((value.frames - 1) / rate, value.frames / rate), "");
        assert.equal(value.end - value.start, 1);
        value.fitSelection();
        assert.equal(value.span, Math.round(rate * 0.01));
        assert.equal(value.right, value.frames);
        assert.deepEqual(value.edit({ kind: "reverse" }), {
            start_seconds: (value.frames - 1) / rate, end_seconds: value.frames / rate, operation: { kind: "reverse" },
        });
    }
});

test("nearest-frame rounding, invalid drafts and sample-exact seconds round trips", () => {
    const value = state();
    assert.equal(value.setSeconds(0.49 / 48000, 1.5 / 48000), "");
    assert.deepEqual([value.start, value.end], [0, 2]);
    for (const times of [[NaN, 1], [0, NaN], [-1, 0.5], [0.7, 0.2], [0, Infinity], [0, 2]]) {
        assert.notEqual(value.setSeconds(...times), "");
        assert.deepEqual([value.start, value.end], [0, 2]);
    }
    for (const rate of [44100, 48000, 192000, 384000])
        for (const frame of [1, 2, 12345, 15875999])
            assert.equal(Math.round(Number(frameText(frame, rate)) * rate), frame);
    assert.equal(value.setSeconds(0.1, 0.1), "");
    assert.equal(value.hasSelection, false);
});

test("reverse drag and sub-frame drags select at least one complete frame", () => {
    const value = state();
    value.selectFrames(20, 10);
    assert.deepEqual([value.start, value.end], [10, 20]);
    value.selectFrames(48000, 48000);
    assert.deepEqual([value.start, value.end], [47999, 48000]);
});

test("zoom/pan stay bounded, fit preserves selection, short tracks fit in full", () => {
    const value = state();
    value.setSeconds(0.2, 0.6); value.fitSelection();
    const original = [value.left, value.right];
    value.pan(1); value.pan(-1);
    assert.deepEqual([value.left, value.right], original);
    for (let i = 0; i < 30; i++) value.zoom(0.5);
    assert.equal(value.span, 480);
    for (let i = 0; i < 200; i++) value.pan(1);
    assert.equal(value.right, value.frames);
    for (let i = 0; i < 30; i++) value.zoom(2);
    assert.deepEqual([value.left, value.right], [0, value.frames]);
    assert.deepEqual([value.start, value.end], [9600, 28800]);
    const short = state(48000, 1); short.zoom(0.5);
    assert.deepEqual([short.left, short.right], [0, 1]);
});

test("same-format immutable revisions preserve range/view; reset clears them", () => {
    const value = state();
    value.setSeconds(0.2, 0.5); value.fitSelection();
    value.configure({ sampleRate: 48000, length: 48000 });
    assert.deepEqual([value.start, value.end, value.left, value.right], [9600, 24000, 9600, 24000]);
    value.reset();
    assert.deepEqual([value.rate, value.frames, value.start, value.end, value.left, value.right], [0, 0, 0, 0, 0, 0]);
});

test("gain is finite and inclusive from -60 to +12; ticks have bounded density", () => {
    for (const db of [-60, -6.125, 0, 12]) assert.equal(gainError(db), "");
    for (const db of [-60.001, 12.001, NaN, Infinity]) assert.notEqual(gainError(db), "");
    for (const frames of [480, 48000, 43200000]) {
        const ticks = rulerTicks(0, frames, 48000, 320);
        assert.ok(ticks.length > 0 && ticks.length <= 5);
        assert.ok(ticks.every((tick) => tick.x >= 0 && tick.x <= 320.00001 && tick.label.endsWith(" s")));
    }
});

test("cached envelopes preserve right-only and opposite-polarity stereo; no repeat channel reads", () => {
    const left = new Float32Array(999), right = new Float32Array(999);
    right[2] = 0.75; right[998] = -0.9; left[256] = 0.5; right[256] = -0.5;
    const source = buffer([left, right]), cache = createEnvelopeCache(), envelope = cache.get(source);
    assert.deepEqual(envelope.range(0, 128), [0, 0.75]);
    assert.deepEqual(envelope.range(256, 257), [-0.5, 0.5]);
    assert.deepEqual(envelope.range(998, 999), [right[998], 0]);
    for (let redraw = 0; redraw < 100; redraw++) cache.get(source).range(0, 999);
    assert.equal(source.reads, 2);
    const replacement = buffer([new Float32Array(999)]);
    assert.deepEqual(cache.get(replacement).range(0, 999), [0, 0]);
    assert.equal(replacement.reads, 1);
    cache.reset(); cache.get(source);
    assert.equal(source.reads, 4);
});

test("hierarchical and zoomed envelope queries match independent brute-force extrema", () => {
    const channels = [new Float32Array(32771), new Float32Array(32771)];
    channels.forEach((channel, index) => channel.forEach((_, frame) => {
        channel[frame] = Math.sin(frame * (0.037 + index * 0.3)) * (frame % 31) / 30;
    }));
    const envelope = createEnvelopeCache().get(buffer(channels));
    const ranges = [[0, 32771], [1, 127], [127, 256], [256, 32768], [32760, 32771], [901.2, 902.1]];
    for (let index = 0; index < 90; index++) ranges.push([index * 211 + 0.3, index * 211 + 10001.7]);
    for (const [start, end] of ranges) {
        let min = 0, max = 0;
        for (const channel of channels)
            for (let frame = Math.floor(start); frame < Math.min(channel.length, Math.ceil(end)); frame++) {
                min = Math.min(min, channel[frame]); max = Math.max(max, channel[frame]);
            }
        assert.deepEqual(envelope.range(start, end), [min, max], `${start}–${end}`);
    }
});
