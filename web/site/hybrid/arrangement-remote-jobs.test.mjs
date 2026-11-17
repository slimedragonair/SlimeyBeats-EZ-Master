import test from "node:test";
import assert from "node:assert/strict";
import { createRemoteArrangementJobs, checkedArrangementArtifact } from "./arrangement-remote-jobs.js";

const base = { epoch: "owned_epoch", revision: 7 };
const record = (state, result) => ({ job_id: "owned_job", operation: "export", epoch: base.epoch,
    base_revision: base.revision, state, progress: state === "completed" ? 1 : .25, ...(result ? { result } : {}) });

test("server jobs retain exact target revision and return metadata without reading audio", async () => {
    const sent = [], progress = [], result = { artifact: { id: "artifact", bytes: 123, name: "preview.mp3", content_type: "audio/mpeg" } };
    const jobs = createRemoteArrangementJobs({ getSnapshot: () => base, wait: async () => {},
        request: async (path, options) => { sent.push({ path, options }); return path === "/jobs" ? record("queued") : record("completed", result); } });
    assert.deepEqual(await jobs.run("export", { format: "mp3" }, { expectedRevision: 7, onProgress: value => progress.push(value) }), result);
    assert.equal(sent[0].options.body.expected_revision, 7); assert.equal(sent[0].options.body.epoch, base.epoch);
    assert.deepEqual(sent.map(value => value.path), ["/jobs", "/jobs/owned_job"]);
    assert.deepEqual(progress, [.25, 1]); assert.equal(checkedArrangementArtifact(result.artifact), result.artifact);
});

test("cancellation during acknowledgement cancels only the acknowledged owned job", async () => {
    const controller = new AbortController(), sent = [];
    const jobs = createRemoteArrangementJobs({ getSnapshot: () => base,
        request: async path => { sent.push(path); if (path === "/jobs") controller.abort(new DOMException("Cancelled", "AbortError")); return record("queued"); } });
    await assert.rejects(jobs.run("export", {}, { signal: controller.signal }), { name: "AbortError" });
    assert.deepEqual(sent, ["/jobs", "/jobs/owned_job/cancel"]);
});

test("stale completed jobs and foreign polling identities cannot replace current work", async () => {
    let latest = base, cancelled = false;
    const jobs = createRemoteArrangementJobs({ getSnapshot: () => latest, wait: async () => {},
        request: async (path) => {
            if (path.endsWith("/cancel")) { cancelled = true; return record("cancelled"); }
            if (path === "/jobs") return record("queued");
            return { ...record("completed", {}), job_id: "foreign_job" };
        } });
    await assert.rejects(jobs.run("export", {}), /invalid arrangement job/); assert.equal(cancelled, true);
    latest = { ...base, revision: 8 };
    await assert.rejects(jobs.run("export", {}, { expectedRevision: 7 }), /changed/);
    for (const invalid of [{ id: "../asset", bytes: 1, name: "x", content_type: "audio/mpeg" }, { id: "asset", bytes: Infinity }])
        assert.throws(() => checkedArrangementArtifact(invalid), /invalid arrangement download/);
});
