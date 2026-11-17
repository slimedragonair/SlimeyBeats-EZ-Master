import test from "node:test";
import assert from "node:assert/strict";
import { createRemoteArrangementAdapter } from "./arrangement-remote-adapter.js";

function server() {
    let revision = 0, active = 0, maximum = 0;
    const sent = [];
    const snapshot = () => ({ epoch: "epoch", revision, arrangement: { sample_rate: 48000, channels: 2, lanes: [], eq: [] },
        assets: [{ id: "asset", name: "Source", frames: 48000, sample_rate: 48000, channels: 1 }],
        defaults: { bands: Array.from({ length: 7 }, () => ({})) }, capabilities: { playback: false },
        preview: { identity: `epoch:${revision}:after`, frames: 0, pending: true, prepared_revision: null } });
    const project = { id: null, caps: { limits: { max_upload_bytes: 1024 } }, media: { dispose() {} }, path: () => "/v2/projects/project/arrangement",
        async connect() {}, async ensure() { this.id = "project"; }, client: { connectionRevision: 1,
            async arrangementRequest(_path, options = {}) {
                active++; maximum = Math.max(maximum, active);
                try {
                    await Promise.resolve(); const body = options.body; sent.push(body || { operation: "read" });
                    if (body && !["create", "stop"].includes(body.operation) && body.expected_revision !== revision) throw Object.assign(new Error("stale"), { status: 409 });
                    if (body?.operation === "edit") revision++;
                    if (body?.operation === "waveform") return { peaks: [[-.1, .1]], start_frame: 0, end_frame: 1 };
                    if (body?.operation === "eq_response") return { frequencies: [20, 20000], gain_db: [0, 0] };
                    return snapshot();
                } finally { active--; }
            } } };
    return { project, sent, maximum: () => maximum };
}
test("metadata publication precedes the next queued request; a stale curve never reaches the server", async () => {
    const state = server(), adapter = createRemoteArrangementAdapter({ remoteProject: state.project });
    await adapter.read();
    const [edit, curve] = await Promise.allSettled([adapter.edit({ kind: "noop" }, { expectedRevision: 0 }), adapter.eqResponse({})]);
    assert.equal(edit.status, "fulfilled"); assert.equal(curve.status, "rejected"); assert.equal(curve.reason.name, "AbortError");
    assert.equal(state.sent.some(value => value.operation === "eq_response"), false);
    await adapter.eqResponse({}); assert.equal(state.sent.at(-1).expected_revision, 1); assert.equal(state.maximum(), 1);
    adapter.clear();
});
test("an immutable source envelope may use the new revision after a queued metadata edit", async () => {
    const state = server(), adapter = createRemoteArrangementAdapter({ remoteProject: state.project }); await adapter.read();
    const [edit, envelope] = await Promise.all([adapter.edit({ kind: "noop" }, { expectedRevision: 0 }),
        adapter.waveform("asset", { start_frame: 0, end_frame: 1, bins: 1 })]);
    assert.equal(edit.revision, 1); assert.deepEqual(envelope.peaks, [[-.1, .1]]);
    assert.equal(state.sent.find(value => value.operation === "waveform").expected_revision, 1); assert.equal(state.maximum(), 1);
    adapter.clear();
});
