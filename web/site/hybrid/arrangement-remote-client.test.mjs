import test from "node:test";
import assert from "node:assert/strict";
import { ServerClient } from "./server-client.js";

const caps = { api_version: 1, operations: ["master"], formats: ["wav24"], profiles: [{ id: "neutral", label: "Neutral" }],
    limits: { max_upload_bytes: 1024, chunk_bytes: 1024, max_jobs: 1, retention_seconds: 3600 },
    privacy: { temporary_storage: true, retention_seconds: 3600 } };
const path = "/v2/projects/project/arrangement";
function harness(reply) {
    const requests = [];
    const client = new ServerClient({ pageUrl: "https://studio.example/current-work/", fetchImpl: async (url, options) => {
        requests.push({ url, options }); return Response.json(url.endsWith("/v1/capabilities") ? caps : reply);
    } });
    return { client, requests };
}
test("only an exact arrangement request admits a snapshot above the legacy 1 MiB limit", async () => {
    const reply = { document: "x".repeat(1024 * 1024 + 1) }, { client, requests } = harness(reply);
    await client.connect("https://api.slimedragon.net", "fixture-owner");
    assert.deepEqual(await client.arrangementRequest(path), reply);
    await assert.rejects(client.remoteRequest(path), /budget/);
    await assert.rejects(client.arrangementRequest(path, { compact: true }), /budget/);
    for (const request of requests) {
        assert.equal(request.options.headers.Authorization, "Bearer fixture-owner");
        assert.equal(request.options.credentials, "omit"); assert.equal(request.options.redirect, "error");
        assert.equal(request.url.includes("fixture-owner"), false);
    }
});
test("arrangement metadata is bounded before dispatch and cannot select another route", async () => {
    const { client, requests } = harness({});
    for (const bad of [path + "?token=x", path + "/streams/id", "/v2/projects/project/private"])
        assert.throws(() => client.arrangementRequest(bad), /path/);
    assert.throws(() => client.arrangementRequest(path, { method: "PUT" }), /path/);
    assert.throws(() => client.arrangementRequest(path, { method: "POST", body: { value: "x".repeat(16 * 1024 * 1024) } }), /16 MiB/);
    assert.equal(requests.length, 0);
});
test("media credentials accept only the exact owned arrangement stream route", () => {
    const { client } = harness({}), sent = [];
    const port = { postMessage: value => sent.push(value) };
    client.replyMediaAccess(port, path + "/streams/stream");
    assert.equal(sent[0].path, path + "/streams/stream");
    for (const bad of [path, path + "/streams/stream?token=x", path + "/stop/stream", path + "/streams/%2f"])
        assert.throws(() => client.replyMediaAccess(port, bad), /path/);
});
