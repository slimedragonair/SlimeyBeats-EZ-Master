import test from "node:test";
import assert from "node:assert/strict";
import { ServerClient } from "./server-client.js";
import { ServerSession } from "./server-session.js";
import { ServerAvailability } from "./server-availability.js";
import { ProcessingController } from "./processing-controller.js";
import { availabilityDetail } from "./studio-view.js";
import { PUBLIC_API_ORIGIN } from "./endpoint-policy.js";

const settings = { profile: "neutral", format: "wav24", mode: "streaming_safe" };
const caps = { api_version: 1, operations: ["master"], formats: ["wav24"], profiles: [{ id: "neutral", label: "Neutral" }],
    limits: { max_upload_bytes: 1024, chunk_bytes: 1024, max_jobs: 4, retention_seconds: 3600 },
    privacy: { temporary_storage: true, retention_seconds: 3600 } };

async function fixture() {
    const calls = [], behavior = { failure: 0 };
    const selected = new File(["abcdefgh"], "preserved-source.wav", { lastModified: 123 });
    let job, sequence = 0, guests = 0;
    const client = new ServerClient({ pageUrl: "https://studio.example/", fetchImpl: async (url, init) => {
        calls.push({ url, ...init });
        if (url.endsWith("capabilities")) return Response.json(caps);
        if (behavior.failure) {
            if (behavior.failure === "network") throw new TypeError("Network failed");
            return new Response(null, { status: behavior.failure });
        }
        if (url.endsWith("/jobs") && init.method === "POST") job = { id: `job-${++sequence}`, state: "uploading",
            uploaded_bytes: 0, total_bytes: selected.size, progress: 0, message: "Ready",
            expires_at: "2026-12-01T00:00:00Z", artifacts: [] };
        if (init.method === "PUT") job.uploaded_bytes += init.body.size;
        if (url.endsWith("/run")) {
            job.state = "completed"; job.progress = 1;
            job.artifacts = [{ id: "wav", name: "master.wav", bytes: 8, content_type: "audio/wav" }];
        }
        if (url.includes("/artifacts/")) return new Response("wavdata!", { headers: { "Content-Type": "audio/wav" } });
        if (init.method === "DELETE") return Response.json({ deleted: true });
        return Response.json(job);
    } });
    const session = new ServerSession({ client });
    const availability = new ServerAvailability({ pageUrl: "https://studio.example/", setTimer: () => 1, clearTimer() {},
        probe: async () => { calls.push({ url: `${PUBLIC_API_ORIGIN}/v1/status`, method: "GET" });
            return { api_version: 1, status: "online", guest_access: true }; } });
    const controller = new ProcessingController({ session, availability, requestGuest: async () => {
        calls.push({ url: `${PUBLIC_API_ORIGIN}/v1/session`, method: "POST" });
        return { token: `guest-${++guests}` };
    } });
    controller.selectMainFile(selected); await controller.setEnabled(true);
    session.consentToUpload(true); await session.submit(settings);
    assert.equal(session.job.state, "completed");
    return { session, controller, availability, selected, calls, behavior };
}

const operations = {
    refresh: (session) => session.refresh(), delete: (session) => session.delete(),
    download: (session) => session.download("wav", () => assert.fail("An inaccessible download must not be saved")),
};

for (const [name, operation] of Object.entries(operations)) for (const status of [401, 404]) {
    test(`${name} HTTP ${status}: explicit forget clears only server access; explicit reconnect works without reload or replay`, async () => {
        const { session, controller, availability, selected, calls, behavior } = await fixture();
        const previous = session.job, source = session.source;
        behavior.failure = status; await operation(session);
        assert.equal(session.job, previous); assert.equal(session.source, source);
        assert.equal(session.unavailableJobStatus, status); assert.equal(session.canForgetUnavailableJob, true);
        assert.match(availabilityDetail(session, controller), /Retry.*does not confirm server deletion/);
        await controller.check();
        assert.equal(calls.filter((call) => call.url.endsWith("/v1/session")).length, 1);
        const before = calls.length;
        assert.equal(controller.forgetUnavailableJob(), true);
        assert.equal(calls.length, before, "Forgetting is local: no DELETE, guest mint, upload or run");
        assert.equal(session.job, null); assert.equal(session.source, null); assert.equal(session.connected, false);
        assert.equal(session.caps, null); assert.equal(session.consent, false); assert.equal(session.canSubmit, false);
        assert.equal(session.file, selected); assert.equal(controller.mainFile, selected);
        assert.equal(controller.enabled, true); assert.equal(availability.operatorAccess, false);
        assert.deepEqual(session.forgottenJob, { endpoint: PUBLIC_API_ORIGIN, expires_at: previous.expires_at });
        assert.match(session.message, /No server deletion was requested or confirmed.*previous reported expiry/);
        await assert.rejects(session.client.get(previous.id), /Connect/);
        assert.equal(calls.length, before, "The retired credential cannot issue another job request");
        behavior.failure = 0;
        await availability.check();
        assert.equal(session.connected, false, "Health recovery must not mint a new credential");
        const reconnect = calls.length; await controller.check();
        assert.deepEqual(calls.slice(reconnect).map((call) => new URL(call.url).pathname), ["/v1/status", "/v1/session", "/v1/capabilities"]);
        assert.equal(calls.at(-1).headers.Authorization, "Bearer guest-2");
        assert.equal(session.file, selected); assert.equal(session.consent, false);
        const connected = calls.length; await session.submit(settings); assert.equal(calls.length, connected);
        session.consentToUpload(true); await session.submit(settings);
        assert.equal(session.job.state, "completed"); assert.notEqual(session.job.id, previous.id);
        assert.equal(calls.filter((call) => call.url.endsWith("/jobs") && call.method === "POST").length, 2);
    });
}

for (const [name, operation] of Object.entries(operations)) for (const failure of [503, "network"]) {
    test(`${name} ${failure}: preserve the retryable job and reject direct Forget`, async () => {
        const { session, controller, selected, calls, behavior } = await fixture();
        const previous = session.job, source = session.source;
        behavior.failure = failure; await operation(session);
        assert.equal(session.job, previous); assert.equal(session.source, source);
        assert.equal(session.file, selected); assert.equal(session.connected, true);
        assert.equal(session.unavailableJobStatus, 0); assert.equal(session.canForgetUnavailableJob, false);
        const count = calls.length; assert.equal(controller.forgetUnavailableJob(), false);
        assert.equal(session.job, previous); assert.equal(calls.length, count); assert.equal(session.forgottenJob, null);
    });
}

test("a successful retry removes the unavailable marker instead of retiring a now-accessible job", async () => {
    const { session, controller, behavior } = await fixture();
    const previous = session.job;
    behavior.failure = 404; await session.refresh(); assert.equal(session.canForgetUnavailableJob, true);
    behavior.failure = 0; await session.refresh();
    assert.equal(session.job.id, previous.id); assert.equal(session.canForgetUnavailableJob, false);
    assert.equal(controller.forgetUnavailableJob(), false); assert.equal(session.forgottenJob, null);
});

test("unavailable jobs can be forgotten after advanced Disconnect while the main source stays in memory", async () => {
    const { session, controller, selected, behavior } = await fixture();
    behavior.failure = 401; await session.refresh(); controller.disconnect();
    assert.equal(session.job !== null, true); assert.equal(session.connected, false);
    assert.equal(controller.forgetUnavailableJob(), true); assert.equal(controller.mainFile, selected);
    behavior.failure = 0; await controller.check();
    assert.equal(session.connected, true); assert.equal(session.file, selected); assert.equal(session.consent, false);
});

// pagehide clears secrets even if this page later returns from BFCache.
test("a disconnected retained job can be retired explicitly without a fake 401 or remote deletion", async () => {
    const { session, controller, calls, behavior, selected } = await fixture();
    const previous = session.job;
    controller.disconnect();
    assert.equal(session.unavailableJobStatus, 0);
    assert.equal(session.canForgetUnavailableJob, true);
    const before = calls.length;
    assert.equal(controller.forgetUnavailableJob(), true);
    assert.equal(calls.length, before);
    assert.equal(session.job, null);
    assert.equal(session.file, null);
    assert.deepEqual(session.forgottenJob, { endpoint: PUBLIC_API_ORIGIN, expires_at: previous.expires_at });
    await controller.check();
    assert.equal(session.connected, true);
    assert.equal(session.file, selected);
    assert.equal(session.consent, false);
    assert.equal(calls.slice(before).some((call) => call.method === "PUT" || call.method === "DELETE" || call.url.endsWith("/run")), false);
});
