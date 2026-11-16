import test from "node:test";
import assert from "node:assert/strict";
import { ServerAvailability, HEALTH_INTERVAL_MS } from "./server-availability.js";
import { publicRequest, guestSession, PUBLIC_REQUEST_TIMEOUT_MS, PublicServerError } from "./public-request.js";
import { availabilityDetail } from "./studio-view.js";
import { PUBLIC_API_ORIGIN } from "./endpoint-policy.js";

const pageUrl = "https://slimedragon.net/current-work/";
const endpoint = PUBLIC_API_ORIGIN;
const online = () => ({ api_version: 1, status: "online", guest_access: true });
const pending = () => { let resolve; const promise = new Promise((r) => { resolve = r; }); return { promise, resolve }; };
function clock() {
    const timers = new Map(); let id = 0;
    return { timers, setTimer: (callback, ms) => { timers.set(++id, { callback, ms }); return id; },
        clearTimer: (key) => timers.delete(key), fire: (key) => { const value = timers.get(key); timers.delete(key); value.callback(); } };
}

test("availability starts inert, checks only on opt-in or explicit request, and schedules at 30 seconds", async () => {
    const timers = clock(); let calls = 0;
    const monitor = new ServerAvailability({ pageUrl, ...timers, probe: async (origin) => { calls++; assert.equal(origin, endpoint); return online(); } });
    assert.equal(calls, 0); assert.equal(monitor.enabled, false); assert.equal(timers.timers.size, 0);
    await monitor.check(); assert.equal(calls, 1); assert.equal(timers.timers.size, 0);
    monitor.setEnabled(true); await monitor.check();
    assert.equal(calls, 2); assert.equal(monitor.state, "online");
    assert.equal([...timers.timers.values()][0].ms, HEALTH_INTERVAL_MS);
    monitor.setEnabled(false); assert.equal(timers.timers.size, 0);
});

test("availability classifies network, timeout, HTTP failures, busy, disabled and incompatible responses", async () => {
    const cases = [[401, "access"], [403, "access"], [429, "busy"], [500, "down"], [503, "down"], [0, "down"],
        [400, "setup"], [404, "setup"], [200, "setup"]];
    for (const [status, state] of cases) {
        const monitor = new ServerAvailability({ pageUrl, probe: async () => { throw new PublicServerError(status); } });
        await monitor.check(); assert.equal(monitor.state, state, `HTTP ${status}`);
    }
    for (const [value, state] of [[{ ...online(), status: "busy" }, "busy"], [{ ...online(), status: "disabled" }, "disabled"],
        [{ ...online(), guest_access: false }, "access"], [{ ...online(), api_version: 2 }, "setup"], [null, "setup"]]) {
        const monitor = new ServerAvailability({ pageUrl, probe: async () => value });
        await monitor.check(); assert.equal(monitor.state, state);
    }
});

test("a reachable disabled API explains public processing is disabled, independently from network failure", async () => {
    const monitor = new ServerAvailability({ pageUrl, probe: async () => ({ ...online(), status: "disabled", guest_access: false }) });
    await monitor.check();
    assert.equal(monitor.state, "disabled"); assert.match(monitor.detail, /reachable.*public processing is disabled/);
    const detail = availabilityDetail({ connected: false }, { enabled: true, availability: monitor });
    assert.match(detail, /No upload can start/); assert.doesNotMatch(detail, /Unable to reach|Not connected for processing yet/);
    monitor.reportFailure(new PublicServerError(503));
    assert.equal(monitor.state, "down"); assert.match(monitor.detail, /Unable to reach/);
});

test("online health alone explains the explicit guest connection step; remembered jobs need their original credential", () => {
    const controller = { enabled: true, availability: { state: "online", detail: "Server responded." } };
    assert.match(availabilityDetail({ connected: false }, controller), /Not connected.*Check & connect.*no audio/);
    assert.doesNotMatch(availabilityDetail({ connected: true }, controller), /Check & connect/);
    assert.match(availabilityDetail({ connected: false, job: {} }, controller), /original credential.*new guest token cannot recover/);
    controller.enabled = false;
    assert.equal(availabilityDetail({ connected: false }, controller), "Server responded.");
});

test("hidden and disabled probes abort and discard stale health; visibility resumes health only", async () => {
    const timers = clock(), first = pending(); let signal, calls = 0;
    const monitor = new ServerAvailability({ pageUrl, ...timers, probe: async (_, options) => {
        calls++; signal = options.signal; return calls === 1 ? first.promise : online();
    } });
    monitor.setEnabled(true); const checking = monitor.check();
    monitor.setVisible(false); assert.equal(signal.aborted, true);
    first.resolve({ ...online(), status: "busy" }); await checking;
    assert.notEqual(monitor.state, "busy"); assert.equal(timers.timers.size, 0);
    await monitor.check(); assert.equal(calls, 1);
    monitor.setVisible(true); await Promise.resolve(); await Promise.resolve();
    assert.equal(calls, 2); assert.equal(monitor.state, "online");
    monitor.setOnline(false); assert.equal(monitor.state, "offline"); assert.equal(timers.timers.size, 0);
    monitor.setEnabled(false); monitor.setOnline(true); assert.equal(calls, 2);
});

test("new health, mode off and endpoint change each reject late probe results", async () => {
    for (const action of ["new", "off", "endpoint"]) {
        const first = pending(); let calls = 0;
        const monitor = new ServerAvailability({ pageUrl, probe: async () => ++calls === 1 ? first.promise : online() });
        const old = monitor.check();
        if (action === "new") await monitor.check();
        if (action === "off") monitor.setEnabled(false);
        if (action === "endpoint") monitor.setEndpoint("https://slimedragon.net");
        first.resolve({ ...online(), status: "busy" }); await old;
        assert.notEqual(monitor.state, "busy");
    }
});

test("an operation failure invalidates older healthy responses until an explicit fresh check", async () => {
    const held = pending(); let calls = 0;
    const monitor = new ServerAvailability({ pageUrl, probe: async () => ++calls === 1 ? held.promise : online() });
    const old = monitor.check();
    monitor.reportFailure(new PublicServerError(503));
    held.resolve(online()); await old;
    assert.equal(monitor.state, "down");
    await monitor.check(); assert.equal(monitor.state, "online");
});

test("five-second public requests bound body reads, abort transport, omit credentials and never echo bodies", async () => {
    const timers = clock(), body = pending(), entered = pending(); let init, cancelled = false;
    const request = publicRequest(endpoint, "/v1/status", { pageUrl, ...timers, fetchImpl: async (_, options) => {
        init = options;
        return { ok: true, headers: new Headers({ "Content-Type": "application/json" }), body: { getReader: () => ({
            read() { entered.resolve(); return body.promise; },
            cancel() { cancelled = true; body.resolve({ done: true }); return Promise.resolve(); }, releaseLock() {},
        }) } };
    } });
    const rejected = assert.rejects(request, /Unable to reach/);
    await entered.promise;
    assert.equal([...timers.timers.values()][0].ms, PUBLIC_REQUEST_TIMEOUT_MS);
    assert.equal(init.credentials, "omit"); assert.equal(init.redirect, "error"); assert.equal(init.cache, "no-store");
    assert.equal(init.headers, undefined); assert.equal(init.body, undefined);
    timers.fire([...timers.timers.keys()][0]); await rejected;
    assert.equal(init.signal.aborted, true); assert.equal(timers.timers.size, 0);
    assert.equal(cancelled, true);
    await assert.rejects(publicRequest(endpoint, "/v1/status", { pageUrl, fetchImpl: async () => { throw new Error("secret"); } }),
        (error) => !error.message.includes("secret"));
});

test("guest access posts only JSON {}, validates tokens and expiry, and retains exact trusted origin policy", async () => {
    let calls = 0;
    const requestGuest = (value) => guestSession(endpoint, { pageUrl, fetchImpl: async (url, init) => {
        calls++; assert.equal(url, `${endpoint}/v1/session`); assert.equal(init.method, "POST");
        assert.equal(init.body, "{}"); assert.deepEqual(init.headers, { "Content-Type": "application/json" });
        return Response.json(value);
    } });
    assert.equal((await requestGuest({ token: "guest-secret", expires_at: "2026-12-01T00:00:00Z" })).token, "guest-secret");
    for (const value of [{}, { token: "unsafe token", expires_at: "2026-12-01" }, { token: "guest", expires_at: "bad" }])
        await assert.rejects(requestGuest(value));
    const before = calls;
    for (const origin of ["https://evil.example", "https://api.slimedragon.net.evil.example", "http://api.slimedragon.net"])
        await assert.rejects(publicRequest(origin, "/v1/status", { pageUrl, fetchImpl: async () => { calls++; } }));
    assert.equal(calls, before);
});
