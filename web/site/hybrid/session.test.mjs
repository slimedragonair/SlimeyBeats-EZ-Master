import test from "node:test";
import assert from "node:assert/strict";
import { ServerSession } from "./server-session.js";
import { ServerClient } from "./server-client.js";
import { CLIENT_CHUNK_BYTES } from "./protocol.js";

const caps = { api_version: 1, operations: ["master"], formats: ["wav24", "mp3"], profiles: [{ id: "neutral", label: "Neutral" }],
    limits: { max_upload_bytes: 2 ** 30, chunk_bytes: 3, max_jobs: 2, retention_seconds: 3600 }, privacy: { temporary_storage: true, retention_seconds: 3600 } };
const options = { profile: "neutral", format: "wav24", mode: "streaming_safe" };
const file = () => new File(["abcdefgh"], "source.wav", { lastModified: 123 });
function harness(onChange) {
    const calls = [];
    let value = { id: "job-1", state: "uploading", uploaded_bytes: 0, total_bytes: 8, progress: 0, message: "Upload", expires_at: "2026-12-01T00:00:00Z", artifacts: [] };
    const behavior = { failPut: false, failCreate: false, failDelete: false, failGet: false };
    const client = new ServerClient({ pageUrl: "https://studio.example", fetchImpl: async (url, init) => {
        calls.push({ url, ...init });
        if (url.endsWith("capabilities")) return Response.json(caps);
        if (init.method === "DELETE") {
            if (behavior.failDelete) throw new Error("offline");
            return Response.json({ deleted: true });
        }
        if (init.method === "POST" && url.endsWith("/jobs")) {
            if (behavior.failCreate) throw new Error("lost create");
            return Response.json(value);
        }
        if (init.method === "PUT") {
            const offset = Number(new URL(url).searchParams.get("offset"));
            assert.equal(offset, value.uploaded_bytes); assert.ok(init.body.size <= caps.limits.chunk_bytes);
            value.uploaded_bytes += init.body.size;
            if (behavior.failPut) { behavior.failPut = false; throw new Error("lost acknowledgement"); }
        }
        if (url.endsWith("/run")) value.state = "queued";
        if (init.method === "GET") {
            if (behavior.failGet) throw new Error("offline");
            if (value.state === "queued") { value.state = "completed"; value.progress = 1; }
        }
        return Response.json(value);
    } });
    const session = new ServerSession({ client, pollMs: 1, onChange });
    return { session, calls, behavior, value };
}
async function ready(session) {
    await session.connect("https://api.slimedragon.net", "token");
    session.select(file()); session.consentToUpload(true);
}

test("default is inert; separate WAV/MP3 selection and consent are enforced even by direct handler calls", async () => {
    const { session, calls } = harness();
    await session.submit(options); assert.equal(calls.length, 0);
    await session.connect("https://api.slimedragon.net", "token");
    session.select(new File(["a"], "source.flac")); assert.equal(session.file, null);
    session.select(file()); await session.submit(options);
    assert.equal(calls.length, 1); assert.equal(session.canSubmit, false);
    session.consentToUpload(true); assert.equal(session.canSubmit, true);
    session.select(file()); assert.equal(session.consent, false);
    session.consentToUpload(true);
    await session.submit({ ...options, format: "flac" }); assert.equal(calls.length, 1);
    session.disconnect(); assert.equal(session.file, null); assert.equal(session.consent, false);
});

test("explicit upload is bounded, exact-appending and followed by explicit run then polling", async () => {
    const { session, calls } = harness(); await ready(session);
    await session.submit(options);
    assert.equal(session.job.state, "completed");
    assert.deepEqual(calls.filter((r) => r.method === "PUT").map((r) => [new URL(r.url).searchParams.get("offset"), r.body.size]), [["0", 3], ["3", 3], ["6", 2]]);
    assert.deepEqual(JSON.parse(calls.find((r) => r.url.endsWith("/jobs")).body), {
        filename: "source.wav", bytes: 8, ...options, target_lufs: null, ceiling_dbtp: -1, intensity: 1, safety: true, reference_analysis: null, consent: true,
    });
    assert.equal(calls.filter((r) => r.url.endsWith("/run")).length, 1);
    assert.equal(session.canSubmit, false);
    await session.submit(options); assert.equal(calls.filter((r) => r.url.endsWith("/run")).length, 1);
});

test("lost chunk acknowledgement resumes from server offset without creating a duplicate job", async () => {
    const { session, calls, behavior } = harness(); await ready(session);
    behavior.failPut = true; await session.submit(options);
    assert.equal(session.error, true); assert.equal(session.job.uploaded_bytes, 0); assert.equal(session.canSubmit, true);
    await session.submit(options);
    assert.equal(session.job.state, "completed");
    assert.equal(calls.filter((r) => r.url.endsWith("/jobs")).length, 1);
    assert.deepEqual(calls.filter((r) => r.method === "PUT").map((r) => new URL(r.url).searchParams.get("offset")), ["0", "3", "6"]);
});

test("failed polling is retryable; failed DELETE never claims success and can be retried", async () => {
    const { session, behavior, calls } = harness(); await ready(session);
    behavior.failGet = true; await session.submit(options);
    assert.equal(session.job.state, "queued"); assert.equal(session.busy, "");
    behavior.failGet = false; await session.refresh(); assert.equal(session.job.state, "completed");
    behavior.failDelete = true; await session.delete();
    assert.equal(session.job.state, "completed"); assert.match(session.message, /NOT confirmed/);
    behavior.failDelete = false; await session.delete();
    assert.equal(session.job, null); assert.equal(session.file, null); assert.equal(session.consent, false);
    assert.equal(calls.filter((r) => r.method === "DELETE").length, 2);
});

test("disconnect clears secrets/selection but preserves destination and expiry for explicit reconnect/delete", async () => {
    const { session, behavior, calls } = harness(); await ready(session);
    behavior.failPut = true; await session.submit(options); session.disconnect();
    assert.equal(session.file, null); assert.equal(session.consent, false); assert.equal(session.connected, false);
    assert.match(session.message, /Deletion has not been confirmed/);
    assert.equal(calls.filter((r) => r.method === "DELETE").length, 0);
    await session.connect("https://studio.example", "token");
    const before = calls.length; await session.delete(); await session.refresh();
    assert.equal(calls.length, before, "Old job cannot be accessed at a new server");
    session.disconnect(); await session.connect("https://api.slimedragon.net", "token");
    session.select(new File(["abcdefgh"], "changed.wav", { lastModified: 123 })); assert.equal(session.file, null);
    session.select(file()); assert.ok(session.file); assert.equal(session.consent, false);
    await session.delete(); assert.equal(session.job, null);
});

test("lost creation response is never retried automatically and warns of an empty orphan job", async () => {
    const { session, behavior, calls } = harness(); await ready(session);
    behavior.failCreate = true; await session.submit(options);
    assert.equal(session.job, null); assert.equal(session.creationUncertain, true);
    assert.match(session.message, /another empty job/); assert.equal(calls.length, 2);
    assert.equal(calls.filter((r) => r.method === "PUT").length, 0);
});

test("large source bypasses local browser guard without decoding; chunks also have a client-side memory bound", async () => {
    const { session } = harness(); await ready(session);
    session.select({ name: "long.mp3", size: 500 * 1024 ** 2, lastModified: 0 });
    assert.equal(session.file.size, 500 * 1024 ** 2);
    assert.equal(CLIENT_CHUNK_BYTES, 4 * 1024 ** 2);
});

test("an oversized advertised chunk cannot cause a whole-file upload", async () => {
    const source = new File([new Uint8Array(9 * 1024 ** 2)], "large.wav", { lastModified: 123 });
    const chunks = [];
    const value = { id: "large-job", state: "uploading", total_bytes: source.size, uploaded_bytes: 0 };
    const client = { endpoint: "https://api.slimedragon.net", cancelPending() {}, disconnect() {},
        connect: async () => ({ ...caps, limits: { ...caps.limits, chunk_bytes: 1024 ** 3 } }),
        create: async () => ({ ...value }), upload: async (id, offset, body) => {
            chunks.push({ offset, bytes: body.size }); value.uploaded_bytes += body.size; return { ...value };
        }, run: async () => ({ ...value, state: "completed" }) };
    const session = new ServerSession({ client }); await session.connect(client.endpoint, "token");
    session.select(source); session.consentToUpload(true); await session.submit(options);
    assert.equal(session.job.state, "completed");
    assert.deepEqual(chunks, [{ offset: 0, bytes: 4194304 }, { offset: 4194304, bytes: 4194304 }, { offset: 8388608, bytes: 1048576 }]);
});

test("explicit DELETE during upload aborts the operation and late replies cannot revive deleted files", async () => {
    let release, entered;
    const pending = new Promise((resolve) => { entered = resolve; });
    const calls = [];
    const client = { endpoint: "https://api.slimedragon.net", cancelPending() { calls.push("abort"); }, disconnect() {},
        connect: async () => caps, create: async () => ({ id: "job-1", state: "uploading", uploaded_bytes: 0, total_bytes: 8 }),
        upload: async () => { entered(); return new Promise((resolve) => { release = resolve; }); },
        delete: async (id) => { calls.push(`DELETE ${id}`); }, run: async () => calls.push("run") };
    const session = new ServerSession({ client }); await ready(session);
    const processing = session.submit(options); await pending; await session.delete();
    assert.equal(session.job, null); assert.equal(session.consent, false);
    release({ id: "job-1", state: "uploading", uploaded_bytes: 3, total_bytes: 8 }); await processing;
    assert.deepEqual(calls, ["abort", "DELETE job-1"]);
    assert.equal(session.job, null); assert.equal(session.busy, "");
});

test("disconnect during a pending upload ignores late responses and never starts a run", async () => {
    let release, entered;
    const pending = new Promise((resolve) => { entered = resolve; });
    const calls = [];
    const client = { endpoint: "https://api.slimedragon.net", cancelPending() {}, disconnect() {},
        connect: async () => caps, create: async () => ({ id: "job-1", state: "uploading", uploaded_bytes: 0, total_bytes: 8 }),
        upload: async () => { entered(); return new Promise((resolve) => { release = resolve; }); },
        run: async () => calls.push("run") };
    const session = new ServerSession({ client }); await ready(session);
    const processing = session.submit(options); await pending; session.disconnect();
    release({ id: "job-1", state: "uploading", uploaded_bytes: 3, total_bytes: 8 }); await processing;
    assert.deepEqual(calls, []); assert.equal(session.connected, false); assert.equal(session.busy, "");
    assert.equal(session.job.uploaded_bytes, 0);
});
