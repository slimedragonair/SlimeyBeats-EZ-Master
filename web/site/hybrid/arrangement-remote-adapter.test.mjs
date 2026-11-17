import test from "node:test";
import assert from "node:assert/strict";
import { createRemoteArrangementAdapter } from "./arrangement-remote-adapter.js";

function server() {
    let revision = 0, active = 0, maximum = 0, epoch = "epoch", prepared = null;
    const sent = [];
    const snapshot = () => ({ epoch, revision, arrangement: { sample_rate: 48000, channels: 2, lanes: [], eq: [] },
        assets: [{ id: "asset", name: "Source", frames: 48000, sample_rate: 48000, channels: 1 }],
        defaults: { bands: Array.from({ length: 7 }, () => ({})) }, capabilities: { playback: false },
        preview: { identity: `${epoch}:${revision}:after`, frames: 0, pending: prepared !== revision, prepared_revision: prepared } });
    const project = { id: null, caps: { arrangement: { available: true, playback: true }, limits: { max_upload_bytes: 1024 } }, media: { dispose() {} }, path: () => "/v2/projects/project/arrangement",
        async connect() {}, async ensure() { this.id = "project"; }, client: { connectionRevision: 1,
            async arrangementRequest(_path, options = {}) {
                active++; maximum = Math.max(maximum, active);
                try {
                    await Promise.resolve(); const body = options.body; sent.push(body || { operation: "read" });
                    if (body && !["create", "stop"].includes(body.operation) && body.expected_revision !== revision) throw Object.assign(new Error("stale"), { status: 409 });
                    if (body?.operation === "edit") revision++;
                    if (body?.operation === "prepare") {
                        prepared = revision;
                        return { stream_id: "stream", stream_path: "/v2/projects/project/arrangement/streams/stream", epoch,
                            prepared_revision: revision, identity: `${epoch}:${revision}:after`, sample_rate: 48000, channels: 2,
                            duration_frames: 0, side_duration_frames: 0, start_frame: 0, side: body.side,
                            preview_sample_rate: 48000, preview_priming_frames: 1105 };
                    }
                    if (body?.operation === "waveform") return { peaks: [[-.1, .1]], start_frame: 0, end_frame: 1 };
                    if (body?.operation === "eq_response") return { frequencies: [20, 20000], gain_db: [0, 0] };
                    return snapshot();
                } finally { active--; }
            } } };
    return { project, sent, maximum: () => maximum, setEpoch: value => epoch = value };
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

test("an unqualified server capability creates no project or arrangement actor", async () => {
    const state = server(); state.project.caps.arrangement.playback = false;
    let projects = 0; state.project.ensure = async () => { projects++; };
    const adapter = createRemoteArrangementAdapter({ remoteProject: state.project });
    await assert.rejects(adapter.read(), /has not enabled arrangement playback/);
    assert.equal(projects, 0); assert.deepEqual(state.sent, []); adapter.clear();
});

test("a browser requiring timestamped MP4 refuses an unqualified server before uploading or creating a project", async () => {
    const original = globalThis.MediaSource;
    globalThis.MediaSource = { isTypeSupported: value => value === 'audio/mp4; codecs="mp4a.6B"' };
    const state = server(); let projects = 0; state.project.ensure = async () => { projects++; };
    const adapter = createRemoteArrangementAdapter({ remoteProject: state.project });
    try {
        await assert.rejects(adapter.read(), /live preview format required by this browser/);
        assert.equal(projects, 0); assert.deepEqual(state.sent, []);
    } finally { adapter.clear(); globalThis.MediaSource = original; }
});

test("a retired stream from a replaced project epoch never sends Stop to the new project", async () => {
    const state = server(), errors = [], adapter = createRemoteArrangementAdapter({ remoteProject: state.project, onError: error => errors.push(error) });
    const source = await adapter.previewSource();
    state.setEpoch("new_epoch"); await adapter.read(); source.stop();
    await new Promise(resolve => setImmediate(resolve)); await adapter.read();
    assert.equal(state.sent.some(value => value.operation === "stop"), false); assert.deepEqual(errors, []);
    adapter.clear();
});
