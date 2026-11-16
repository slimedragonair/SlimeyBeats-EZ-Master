import test from "node:test";
import assert from "node:assert/strict";
import { serverEndpoint } from "./endpoint-policy.js";
import { capabilities, snapshot, artifactFilename } from "./protocol.js";
import { ServerClient } from "./server-client.js";

export const CAPS = { api_version: 1, operations: ["master"], formats: ["wav24", "mp3"],
    profiles: [{ id: "neutral", label: "Neutral" }], limits: { max_upload_bytes: 2 ** 30, chunk_bytes: 3, max_jobs: 2, retention_seconds: 3600 },
    privacy: { temporary_storage: true, retention_seconds: 3600 } };
export const job = (state = "uploading", uploaded_bytes = 0) => ({ id: "job-1", state, uploaded_bytes, total_bytes: 8,
    progress: state === "completed" ? 1 : 0, message: state, expires_at: "2026-12-01T00:00:00Z", artifacts: [] });
const pageUrl = "https://studio.example/current-work/";

test("endpoint is empty by default; exact production allowlist and local-page-only HTTP loopback", () => {
    for (const value of ["", "https://evil.example", "http://api.slimedragon.net", "https://api.slimedragon.net.evil.example",
        "https://token@api.slimedragon.net", "https://api.slimedragon.net?token=x", "https://api.slimedragon.net/#x", "https://api.slimedragon.net/v1",
        "https://api.slimedragon.net:444", "http://localhost:8000", "http://127.0.0.1:8000"])
        assert.throws(() => serverEndpoint(value, pageUrl), undefined, value);
    assert.equal(serverEndpoint("https://api.slimedragon.net/", pageUrl), "https://api.slimedragon.net");
    assert.equal(serverEndpoint("https://studio.example", pageUrl), "https://studio.example");
    for (const endpoint of ["http://127.0.0.1:9000", "http://localhost:9000", "http://[::1]:9000"])
        assert.equal(serverEndpoint(endpoint, "http://localhost:8000/"), endpoint);
    assert.throws(() => serverEndpoint("http://192.168.1.1:9000", "http://localhost:8000/"));
});

test("incompatible and contradictory retention policies, limits and snapshots fail closed", () => {
    assert.equal(capabilities(CAPS), CAPS);
    for (const change of [{ api_version: 2 }, { operations: [] }, { formats: ["flac"] }, { profiles: [] },
        { limits: { ...CAPS.limits, chunk_bytes: 0 } }, { privacy: { temporary_storage: false, retention_seconds: 3600 } },
        { privacy: { temporary_storage: true, retention_seconds: 7200 } }])
        assert.throws(() => capabilities({ ...CAPS, ...change }));
    for (const change of [{ id: ".." }, { state: "unknown" }, { total_bytes: -1 }, { uploaded_bytes: 9 },
        { progress: NaN }, { expires_at: null }, { artifacts: [{ id: "../x", name: "x", bytes: 8, content_type: "audio/wav" }] }])
        assert.throws(() => snapshot({ ...job(), ...change }));
    assert.throws(() => snapshot(job(), "another-job"));
});

test("optional duration and decoded PCM limits accept positive safe integers and reject malformed values when present", () => {
    assert.equal(capabilities(CAPS), CAPS, "Older capability responses remain compatible");
    for (const key of ["max_audio_seconds", "max_decoded_bytes"]) {
        const valid = { ...CAPS, limits: { ...CAPS.limits, [key]: key === "max_audio_seconds" ? 1800 : 512 * 1024 ** 2 } };
        assert.equal(capabilities(valid), valid);
        for (const invalid of [undefined, null, 0, -1, 1.5, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1, "1800", true, {}, []])
            assert.throws(() => capabilities({ ...CAPS, limits: { ...CAPS.limits, [key]: invalid } }), undefined, `${key}: ${String(invalid)}`);
    }
    const both = { ...CAPS, limits: { ...CAPS.limits, max_audio_seconds: 1800, max_decoded_bytes: 512 * 1024 ** 2 } };
    assert.equal(capabilities(both), both);
});

test("only explicit connect fetches; all API calls authenticate without cookies, redirects or token URLs", async () => {
    const requests = [];
    const client = new ServerClient({ pageUrl, fetchImpl: async (url, init) => {
        requests.push({ url, init });
        return Response.json(url.endsWith("capabilities") ? CAPS : job());
    } });
    assert.equal(requests.length, 0);
    await client.connect("https://api.slimedragon.net", "private-token");
    assert.equal(requests.length, 1);
    await client.create({ consent: true });
    await client.upload("job-1", 0, new Blob(["abc"]));
    await client.run("job-1"); await client.get("job-1");
    for (const { url, init } of requests) {
        assert.equal(init.headers.Authorization, "Bearer private-token");
        assert.equal(init.credentials, "omit"); assert.equal(init.cache, "no-store");
        assert.equal(init.redirect, "error"); assert.equal(init.referrerPolicy, "no-referrer");
        assert.ok(!url.includes("private-token"));
    }
    client.disconnect();
    await assert.rejects(client.get("job-1"), /Connect/);
    assert.equal(JSON.stringify(client).includes("private-token"), false);
});

test("disconnect aborts pending fetch and body consumption; lost old connect cannot clear a new token", async () => {
    const pending = [];
    const client = new ServerClient({ pageUrl, fetchImpl: async (url, init) => {
        if (init.headers.Authorization === "Bearer new-token") return Response.json(url.endsWith("capabilities") ? CAPS : job());
        return new Promise((resolve, reject) => pending.push({ init, reject, resolve }));
    } });
    const old = client.connect("https://api.slimedragon.net", "old-token");
    client.disconnect();
    assert.equal(pending[0].init.signal.aborted, true);
    await client.connect("https://api.slimedragon.net", "new-token");
    pending[0].reject(new Error("aborted"));
    await assert.rejects(old);
    await client.get("job-1");
    let bodySignal, started, bodyCancelled = false;
    const ready = new Promise((resolve) => { started = resolve; });
    const reader = new ServerClient({ pageUrl, fetchImpl: async (url, init) => {
        if (url.endsWith("capabilities")) return Response.json(CAPS);
        bodySignal = init.signal;
        return { ok: true, headers: new Headers({ "Content-Type": "application/json" }), body: { getReader: () => ({
            read: () => new Promise((resolve, reject) => {
                init.signal.addEventListener("abort", () => reject(new Error("body aborted"))); started();
            }),
            cancel: async () => { bodyCancelled = true; }, releaseLock() {},
        }) } };
    } });
    await reader.connect("https://api.slimedragon.net", "token");
    const reading = reader.get("job-1"); await ready;
    reader.disconnect(); assert.equal(bodySignal.aborted, true); await assert.rejects(reading, /stopped/);
    assert.equal(bodyCancelled, true);
});

test("download is authenticated and safely named; DELETE requires acknowledgement and errors never echo response bodies", async () => {
    const requests = [];
    let deleteAck = false;
    const client = new ServerClient({ pageUrl, fetchImpl: async (url, init) => {
        requests.push({ url, init });
        if (url.endsWith("capabilities")) return Response.json(CAPS);
        if (init.method === "DELETE") return Response.json({ deleted: deleteAck });
        return new Response("wavdata!", { headers: { "Content-Type": "audio/wav", "Content-Disposition": 'attachment; filename="../../token.html"' } });
    } });
    await client.connect("https://api.slimedragon.net", "secret");
    const result = await client.artifact("job-1", { id: "wav", name: "master.wav", bytes: 8, content_type: "audio/wav" });
    assert.equal(result.name, "server-token.wav"); assert.equal(result.blob.size, 8);
    assert.equal(requests.at(-1).init.headers.Authorization, "Bearer secret");
    await assert.rejects(client.delete("job-1"), /not confirmed/);
    deleteAck = true; await client.delete("job-1");
    assert.equal(requests.at(-1).init.method, "DELETE");
    const artifact = { name: "report.json", content_type: "application/json" };
    assert.equal(artifactFilename(artifact, "attachment; filename*=UTF-8''%2F%2Fevil%2Freport.exe"), "server-report.json");
    assert.equal(artifactFilename(artifact, "attachment; filename*=UTF-8''%INVALID"), "server-report.json");
    assert.throws(() => artifactFilename({ ...artifact, content_type: "text/html" }));
    assert.throws(() => artifactFilename({ ...artifact, content_type: "constructor" }));
    const denied = new ServerClient({ pageUrl, fetchImpl: async () => new Response("secret", { status: 401 }) });
    await assert.rejects(denied.connect("https://api.slimedragon.net", "secret"), (error) => /401/.test(error.message) && !error.message.includes("secret"));
});
