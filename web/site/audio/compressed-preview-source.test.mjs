import test from "node:test";
import assert from "node:assert/strict";
import { createCompressedPreview, COMPRESSED_PREVIEW_MIME } from "./compressed-preview-source.js";

async function until(predicate) {
    const deadline = Date.now() + 1500;
    while (!predicate()) { assert.ok(Date.now() < deadline, "buffer operation timed out"); await new Promise(resolve => setTimeout(resolve, 5)); }
}

function fixture({ chunks = 40, mime = "audio/mp4" } = {}) {
    const saved = { MediaSource: globalThis.MediaSource, fetch: globalThis.fetch,
        create: URL.createObjectURL, revoke: URL.revokeObjectURL };
    const state = { reads: 0, cancelled: 0, revoked: 0, errors: [], operations: [], audio: { currentTime: 0 } };
    class Buffer extends EventTarget {
        start = 0; end = 0; updating = false;
        get buffered() { return { length: this.end > this.start ? 1 : 0, start: () => this.start, end: () => this.end }; }
        update(action) {
            assert.equal(this.updating, false, "append and remove must be serialized"); this.updating = true;
            queueMicrotask(() => { action(); this.updating = false; this.dispatchEvent(new Event("updateend")); });
        }
        appendBuffer() { state.operations.push("append"); this.update(() => this.end += .25); }
        remove(_start, end) { state.operations.push("remove"); this.update(() => this.start = end); }
    }
    globalThis.MediaSource = class extends EventTarget {
        readyState = "open";
        constructor() { super(); state.media = this; }
        addSourceBuffer(mime) { assert.equal(mime, COMPRESSED_PREVIEW_MIME); return state.buffer = new Buffer(); }
        endOfStream() { this.readyState = "ended"; }
    };
    URL.createObjectURL = () => { queueMicrotask(() => state.media.dispatchEvent(new Event("sourceopen"))); return "blob:owned"; };
    URL.revokeObjectURL = () => state.revoked++;
    globalThis.fetch = async (_url, options) => {
        state.signal = options.signal;
        return { ok: true, headers: new Headers({ "Content-Type": mime }), body: {
            async cancel() { state.cancelled++; },
            getReader: () => ({ async read() { return ++state.reads <= chunks ? { value: new Uint8Array(8) } : { done: true }; },
                async cancel() { state.cancelled++; }, releaseLock() {} }) } };
    };
    state.restore = () => { globalThis.MediaSource = saved.MediaSource; globalThis.fetch = saved.fetch;
        URL.createObjectURL = saved.create; URL.revokeObjectURL = saved.revoke; };
    return state;
}

test("timestamped compressed preview bounds paused reads, serializes eviction, and cancels owned resources", async () => {
    const state = fixture(), preview = createCompressedPreview(state.audio, "/owned", 10, error => state.errors.push(error));
    try {
        await preview.ready; await until(() => state.reads === 6);
        assert.equal(state.buffer.mode, "segments");
        await new Promise(resolve => setTimeout(resolve, 100)); assert.equal(state.reads, 6);
        state.audio.currentTime = 4; await until(() => state.operations.includes("remove"));
        assert.equal(state.buffer.start, 2); assert.ok(state.buffer.end <= 5.25);
        preview.dispose(); await until(() => state.signal.aborted && state.cancelled > 0);
        assert.equal(state.revoked, 1); assert.deepEqual(state.errors, []);
    } finally { preview.dispose(); state.restore(); }
});

test("short compressed playback reaches EOF and truncated playback fails explicitly", async () => {
    for (const [duration, failed] of [[.25, false], [1, true]]) {
        const state = fixture({ chunks: 1 }), preview = createCompressedPreview(state.audio, "/owned", duration, error => state.errors.push(error));
        try {
            if (failed) { await assert.rejects(preview.ready, /ended before/); assert.equal(state.errors.length, 1); }
            else { await preview.ready; await until(() => state.media.readyState === "ended"); assert.deepEqual(state.errors, []); }
        } finally { preview.dispose(); state.restore(); }
    }
});

test("a mismatched response type never reaches a decoder buffer", async () => {
    const state = fixture({ mime: "text/html" }), preview = createCompressedPreview(state.audio, "/owned", 1, error => state.errors.push(error));
    try {
        await assert.rejects(preview.ready, /stream is unavailable/);
        assert.equal(state.reads, 0); assert.deepEqual(state.operations, []); assert.equal(state.cancelled, 1);
    } finally { preview.dispose(); state.restore(); }
});
