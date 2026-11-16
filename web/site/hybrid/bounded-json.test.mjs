import test from "node:test";
import assert from "node:assert/strict";
import { readBoundedJson, PUBLIC_JSON_LIMIT_BYTES, AUTHENTICATED_JSON_LIMIT_BYTES } from "./bounded-json.js";
import { publicRequest } from "./public-request.js";
import { ServerClient } from "./server-client.js";

const endpoint = "https://api.slimedragon.net", pageUrl = "https://slimedragon.net/current-work/";
const encoder = new TextEncoder();
function fixture({ chunks = [encoder.encode("{}")], headers = { "Content-Type": "application/json" } } = {}) {
    const seen = { reads: 0, cancelled: 0, released: 0, fallbacks: 0, readers: 0 };
    const reader = {
        async read() { const value = chunks[seen.reads++]; return value ? { value, done: false } : { done: true }; },
        async cancel() { seen.cancelled++; }, releaseLock() { seen.released++; },
    };
    const response = { ok: true, status: 200, headers: new Headers(headers), body: {
        getReader() { seen.readers++; return reader; }, async cancel() { seen.cancelled++; },
    } };
    for (const name of ["json", "text", "arrayBuffer", "blob"]) response[name] = async () => { seen.fallbacks++; throw new Error("Unbounded fallback called"); };
    return { seen, response, reader };
}

test("public status/session JSON reject oversized or malformed declared lengths and non-JSON MIME before reading", async () => {
    for (const path of ["/v1/status", "/v1/session"]) {
        const headers = [
            { "Content-Type": "text/html" }, {},
            ...[String(PUBLIC_JSON_LIMIT_BYTES + 1), "-1", "1.5", "NaN", "", "9007199254740992"].map((value) => ({
                "Content-Type": "application/json", "Content-Length": value,
            })),
        ];
        for (const header of headers) {
            const { response, seen } = fixture({ headers: header });
            await assert.rejects(publicRequest(endpoint, path, { pageUrl, method: path.endsWith("session") ? "POST" : "GET", fetchImpl: async () => response }));
            assert.equal(seen.reads, 0); assert.equal(seen.readers, 0); assert.equal(seen.cancelled, 1); assert.equal(seen.fallbacks, 0);
        }
    }
});

test("chunked public JSON exceeding 16 KiB is cancelled before parsing, even with a misleading small Content-Length", async () => {
    for (const path of ["/v1/status", "/v1/session"]) for (const length of [undefined, "2"]) {
        const { response, seen } = fixture({ chunks: [new Uint8Array(PUBLIC_JSON_LIMIT_BYTES), new Uint8Array(1)],
            headers: { "Content-Type": "application/json", ...(length ? { "Content-Length": length } : {}) } });
        await assert.rejects(publicRequest(endpoint, path, { pageUrl, fetchImpl: async () => response }), /exceeded.*budget/);
        assert.equal(seen.cancelled, 1); assert.equal(seen.reads, 2); assert.equal(seen.released, 1); assert.equal(seen.fallbacks, 0);
    }
});

test("exact-budget tiny chunks use one preallocated buffer and preserve split UTF-8 without retained chunk arrays", async () => {
    const text = "é" + "x".repeat(PUBLIC_JSON_LIMIT_BYTES - 13);
    const source = encoder.encode(JSON.stringify({ text }));
    assert.equal(source.byteLength, PUBLIC_JSON_LIMIT_BYTES);
    const original = globalThis.Uint8Array, allocations = [];
    let offset = 0, released = false;
    const response = { headers: new Headers({ "Content-Type": "application/json; charset=utf-8", "Content-Length": String(source.byteLength) }), body: { getReader: () => ({
        async read() {
            assert.deepEqual(allocations, [PUBLIC_JSON_LIMIT_BYTES], "Allocate the fixed buffer before reading the first chunk");
            return offset < source.byteLength ? { done: false, value: source.subarray(offset, ++offset) } : { done: true };
        },
        async cancel() { assert.fail("Exact valid JSON must not be cancelled"); }, releaseLock() { released = true; },
    }) } };
    globalThis.Uint8Array = new Proxy(original, { construct(target, args) { allocations.push(args[0]); return Reflect.construct(target, args); } });
    try { assert.deepEqual(await readBoundedJson(response, { maxBytes: PUBLIC_JSON_LIMIT_BYTES }), { text }); }
    finally { globalThis.Uint8Array = original; }
    assert.deepEqual(allocations, [PUBLIC_JSON_LIMIT_BYTES]); assert.equal(released, true);
});

test("malformed JSON, invalid UTF-8 and unavailable streaming fail closed without echoing bodies or falling back", async () => {
    for (const chunks of [[encoder.encode('{"secret-token":')], [new Uint8Array([0xff])]]) {
        const { response, seen } = fixture({ chunks });
        await assert.rejects(readBoundedJson(response), (error) => /malformed JSON/.test(error.message) && !error.message.includes("secret-token"));
        assert.equal(seen.cancelled, 1); assert.equal(seen.fallbacks, 0);
    }
    const { response, seen } = fixture(); delete response.body.getReader;
    await assert.rejects(readBoundedJson(response), /streaming is unavailable/);
    assert.equal(seen.cancelled, 1); assert.equal(seen.fallbacks, 0);
});

test("authenticated capabilities and upload acknowledgements share the 64 KiB bound", async () => {
    const caps = { api_version: 1, operations: ["master"], formats: ["wav24"], profiles: [{ id: "neutral", label: "Neutral" }],
        limits: { max_upload_bytes: 100, chunk_bytes: 3, max_jobs: 1, retention_seconds: 3600 },
        privacy: { temporary_storage: true, retention_seconds: 3600 }, notes: "x".repeat(PUBLIC_JSON_LIMIT_BYTES + 1) };
    const cases = [fixture({ headers: { "Content-Type": "application/json", "Content-Length": String(AUTHENTICATED_JSON_LIMIT_BYTES + 1) } }),
        fixture({ chunks: [new Uint8Array(AUTHENTICATED_JSON_LIMIT_BYTES + 1)] })];
    for (const value of cases) {
        const client = new ServerClient({ pageUrl, fetchImpl: async (url) => url.endsWith("capabilities") ? Response.json(caps) : value.response });
        assert.equal((await client.connect(endpoint, "owner-token")).notes.length, PUBLIC_JSON_LIMIT_BYTES + 1);
        await assert.rejects(client.upload("job", 0, new Blob(["abc"])), /budget/);
        assert.equal(value.seen.cancelled, 1); assert.equal(value.seen.fallbacks, 0);
        const denied = new ServerClient({ pageUrl, fetchImpl: async () => fixture({ headers: {
            "Content-Type": "application/json", "Content-Length": String(AUTHENTICATED_JSON_LIMIT_BYTES + 1),
        } }).response });
        await assert.rejects(denied.connect(endpoint, "owner-token"), /budget/);
    }
});
