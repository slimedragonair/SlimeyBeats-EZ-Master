import test from "node:test";
import assert from "node:assert/strict";
import { ProcessingController } from "./processing-controller.js";
import { ServerSession } from "./server-session.js";
import { ServerClient } from "./server-client.js";
import { ServerAvailability } from "./server-availability.js";

const endpoint = "https://api.slimedragon.net", pageUrl = "https://slimedragon.net/current-work/";
const caps = { api_version: 1, operations: ["master"], formats: ["wav24", "mp3"], profiles: [{ id: "neutral", label: "Neutral" }],
    limits: { max_upload_bytes: 2 ** 30, chunk_bytes: 3, max_jobs: 2, retention_seconds: 3600 },
    privacy: { temporary_storage: true, retention_seconds: 3600 } };
const options = { profile: "neutral", format: "wav24", mode: "streaming_safe" };
const pending = () => { let resolve; const promise = new Promise((r) => { resolve = r; }); return { promise, resolve }; };
const file = () => new File(["abcdefgh"], "main.wav", { lastModified: 123 });
function fixture(extra = {}) {
    const calls = [], behavior = { offline: false, busy: false, disabled: false };
    const value = { id: "guest-job", state: "uploading", uploaded_bytes: 0, total_bytes: 8, progress: 0, message: "Upload",
        expires_at: "2026-12-01T00:00:00Z", artifacts: [] };
    const client = new ServerClient({ pageUrl, fetchImpl: async (url, init) => {
        calls.push({ url, ...init });
        if (url.endsWith("capabilities")) return Response.json(caps);
        if (init.method === "DELETE") return Response.json({ deleted: true });
        if (init.method === "PUT") { value.uploaded_bytes += init.body.size; }
        if (url.endsWith("/run")) { value.state = "completed"; value.progress = 1; }
        return Response.json(value);
    } });
    const session = new ServerSession({ client });
    const availability = new ServerAvailability({ pageUrl, setTimer: () => 1, clearTimer: () => {}, probe: async () => {
        calls.push({ url: `${endpoint}/v1/status`, method: "GET" });
        if (behavior.offline) throw new Error("offline");
        return { api_version: 1, status: behavior.disabled ? "disabled" : behavior.busy ? "busy" : "online", guest_access: !behavior.disabled };
    } });
    const controller = new ProcessingController({ session, availability, requestGuest: async () => {
        calls.push({ url: `${endpoint}/v1/session`, method: "POST" });
        return { token: "scoped-guest-token", expires_at: "2026-12-01T00:00:00Z" };
    }, ...extra });
    return { controller, session, calls, behavior, availability, value };
}
const uploads = (calls) => calls.filter((call) => call.method === "PUT" || /\/jobs(?:\?|$)|\/run$/.test(call.url));

test("public-disabled mode never acquires a guest or submits audio, including direct consent and submit handlers", async () => {
    const { controller, session, calls, behavior } = fixture(); behavior.disabled = true;
    controller.selectMainFile(file()); await controller.setEnabled(true);
    assert.equal(controller.availability.state, "disabled"); assert.equal(session.available, false);
    session.consentToUpload(true); await session.submit(options);
    assert.equal(session.consent, false); assert.equal(session.canSubmit, false); assert.equal(session.connected, false);
    assert.deepEqual(calls.map((call) => new URL(call.url).pathname), ["/v1/status"]);
});

test("disabled status cancels pending guest issuance and late access cannot connect or start audio", async () => {
    const held = pending(), entered = pending(); let signal;
    const { controller, session, calls, behavior, availability } = fixture({ requestGuest: async (_, options) => {
        signal = options.signal; entered.resolve(); return held.promise;
    } });
    const enabling = controller.setEnabled(true); await entered.promise;
    behavior.disabled = true; await availability.check();
    assert.equal(signal.aborted, true); assert.equal(controller.connecting, false);
    held.resolve({ token: "late-guest" }); await enabling;
    assert.equal(session.connected, false); assert.equal(availability.state, "disabled");
    assert.equal(calls.some((call) => call.url.endsWith("capabilities")), false);
    assert.equal(uploads(calls).length, 0);
});

test("disabled status revokes an existing guest's consent and prevents direct submission while retaining its credential", async () => {
    const { controller, session, behavior, availability, calls } = fixture();
    controller.selectMainFile(file()); await controller.setEnabled(true); session.consentToUpload(true);
    assert.equal(session.canSubmit, true);
    behavior.disabled = true; await availability.check();
    const count = calls.length; session.consentToUpload(true); await session.submit(options);
    assert.equal(session.connected, true); assert.equal(session.available, false); assert.equal(session.consent, false);
    assert.equal(calls.length, count); assert.equal(uploads(calls).length, 0);
});

test("initial outage then periodic health recovery requires an explicit Check to connect, never audio", async () => {
    const { controller, session, calls, behavior, availability } = fixture();
    controller.selectMainFile(file()); behavior.offline = true; await controller.setEnabled(true);
    assert.equal(availability.state, "down"); assert.equal(session.connected, false);
    behavior.offline = false; await availability.check();
    assert.equal(availability.state, "online"); assert.equal(session.connected, false);
    assert.equal(calls.some((call) => call.url.endsWith("/session")), false);
    await controller.check(); assert.equal(session.connected, true); assert.equal(session.file.name, "main.wav");
    assert.equal(session.consent, false); assert.equal(uploads(calls).length, 0);
});

test("fresh mode is local; opt-in acquires guest and authorized capabilities with the current file but never uploads", async () => {
    const { controller, session, calls } = fixture();
    assert.equal(controller.enabled, false); assert.equal(session.canSubmit, false); assert.equal(calls.length, 0);
    const selected = file(); controller.selectMainFile(selected);
    await controller.setEnabled(true);
    assert.equal(session.file, selected); assert.equal(session.connected, true); assert.equal(session.consent, false);
    assert.deepEqual(calls.map((call) => new URL(call.url).pathname), ["/v1/status", "/v1/session", "/v1/capabilities"]);
    await session.submit(options); assert.equal(uploads(calls).length, 0);
    session.consentToUpload(true); await session.submit(options); assert.equal(session.job.state, "completed");
});

test("mode off retains guest credentials, job, source and expiry; retrieval/deletion stay authorized without re-minting", async () => {
    const { controller, session, calls } = fixture(); controller.selectMainFile(file());
    await controller.setEnabled(true); session.consentToUpload(true); await session.submit(options);
    const job = session.job, source = session.source;
    await controller.setEnabled(false);
    assert.equal(session.job, job); assert.equal(session.source, source); assert.equal(session.connected, true);
    assert.equal(session.consent, false); assert.equal(session.canSubmit, false);
    const count = calls.length; session.consentToUpload(true); await session.submit(options); assert.equal(calls.length, count);
    await session.refresh(); await session.delete(); assert.equal(session.job, null);
    assert.equal(calls.at(-1).headers.Authorization, "Bearer scoped-guest-token");
    assert.equal(calls.filter((call) => call.url.endsWith("/session")).length, 1);
});

test("outage and recovery preserve the job; health recovery never uploads, runs, polls or grants consent", async () => {
    const { controller, session, calls, behavior } = fixture(); controller.selectMainFile(file());
    await controller.setEnabled(true); session.consentToUpload(true); await session.submit(options);
    const job = session.job;
    behavior.offline = true; await controller.check();
    assert.equal(controller.availability.state, "down"); assert.equal(session.job, job); assert.equal(session.consent, false);
    behavior.offline = false; const count = calls.length; await controller.check();
    assert.equal(controller.availability.state, "online"); assert.equal(session.job, job);
    assert.deepEqual(calls.slice(count).map((call) => new URL(call.url).pathname), ["/v1/status"]);
    assert.equal(session.consent, false); assert.equal(session.connected, true);
});

test("mode off during upload aborts requests and a late acknowledgement cannot start the server run", async () => {
    const held = pending(), entered = pending(); const { controller, session, calls } = fixture();
    session.client.upload = async () => { entered.resolve(); return held.promise; };
    controller.selectMainFile(file()); await controller.setEnabled(true); session.consentToUpload(true);
    const submitting = session.submit(options); await entered.promise;
    await controller.setEnabled(false);
    held.resolve({ ...session.job, uploaded_bytes: 3 }); await submitting;
    assert.equal(session.job.state, "uploading"); assert.equal(session.job.uploaded_bytes, 0);
    assert.equal(session.connected, true); assert.equal(session.busy, "");
    assert.equal(calls.some((call) => call.url.endsWith("/run")), false);
});

test("mode off cancels late guest session issuance; a hidden health check never silently creates a guest", async () => {
    const held = pending(), entered = pending(); let signal;
    const { controller, session, calls } = fixture({ requestGuest: async (_, options) => {
        signal = options.signal; entered.resolve(); return held.promise;
    } });
    const enabling = controller.setEnabled(true); await entered.promise;
    await controller.setEnabled(false); assert.equal(signal.aborted, true);
    held.resolve({ token: "late-guest" }); await enabling;
    assert.equal(session.connected, false); assert.equal(calls.some((call) => call.url.endsWith("capabilities")), false);
});

test("current long main file is staged without decoding, and busy health does not interrupt an existing operation", async () => {
    const { controller, session, behavior } = fixture();
    const long = { name: "long.mp3", size: 500 * 1024 ** 2, lastModified: 123,
        arrayBuffer() { throw new Error("must not decode"); } };
    controller.selectMainFile(long); await controller.setEnabled(true); assert.equal(session.file, long);
    session.busy = "Checking server job…"; behavior.busy = true; await controller.check();
    assert.equal(controller.availability.state, "busy"); assert.equal(session.busy, "Checking server job…");
    assert.equal(session.canSubmit, false); session.busy = "";
});

test("explicit Disconnect clears credentials, and a remembered guest job prevents minting an unrelated replacement token", async () => {
    const { controller, session, calls } = fixture(); controller.selectMainFile(file());
    await controller.setEnabled(true); session.consentToUpload(true); await session.submit(options);
    controller.disconnect(); assert.equal(session.connected, false); assert.equal(session.file, null);
    const count = calls.length; await controller.check();
    assert.deepEqual(calls.slice(count).map((call) => new URL(call.url).pathname), ["/v1/status"]);
    assert.ok(session.job); assert.equal(session.canSubmit, false);
});
