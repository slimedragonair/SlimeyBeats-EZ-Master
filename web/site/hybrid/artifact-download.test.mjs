import test from "node:test";
import assert from "node:assert/strict";
import { ServerClient } from "./server-client.js";
import { BROWSER_DOWNLOAD_BUDGET_BYTES, checkArtifactDownload } from "./artifact-download.js";

const CAPS = { api_version: 1, operations: ["master"], formats: ["wav24", "mp3"], profiles: [{ id: "neutral", label: "Neutral" }],
    limits: { max_upload_bytes: 1024 ** 3, chunk_bytes: 3, max_jobs: 2, retention_seconds: 3600 },
    privacy: { temporary_storage: true, retention_seconds: 3600 } };
const artifact = (bytes = 8, content_type = "audio/wav") => ({ id: "audio", name: "master.wav", bytes, content_type });

async function clientFor(responder) {
    const requests = [];
    const client = new ServerClient({ pageUrl: "https://studio.example", fetchImpl: async (url, init) => {
        requests.push({ url, init });
        return url.endsWith("capabilities") ? Response.json(CAPS) : responder(url, init);
    } });
    await client.connect("https://api.slimedragon.net", "private-token");
    return { client, requests };
}

function responseFor(chunks, type = "audio/wav") {
    const state = { reads: 0, cancelled: false };
    const body = new ReadableStream({
        pull(controller) {
            const chunk = chunks[state.reads++];
            if (chunk) controller.enqueue(chunk);
            else controller.close();
        },
        cancel() { state.cancelled = true; },
    }, { highWaterMark: 0 });
    const response = new Response(body, { headers: { "Content-Type": type, "Content-Disposition": 'attachment; filename="../../master.html"' } });
    for (const method of ["blob", "arrayBuffer", "text"])
        response[method] = () => assert.fail(`Unbounded response.${method} must never be called`);
    return { response, state };
}

test("oversized or invalid artifact metadata is rejected before an artifact GET", async () => {
    const { client, requests } = await clientFor(() => assert.fail("No artifact fetch expected"));
    assert.equal(BROWSER_DOWNLOAD_BUDGET_BYTES, 134217728);
    assert.doesNotThrow(() => checkArtifactDownload(artifact(BROWSER_DOWNLOAD_BUDGET_BYTES)));
    await assert.rejects(client.artifact("job-1", artifact(BROWSER_DOWNLOAD_BUDGET_BYTES + 1)), /128 MiB download budget/);
    for (const size of [undefined, null, -1, NaN, Infinity, 1.5, "8"])
        await assert.rejects(client.artifact("job-1", { ...artifact(), bytes: size }), /invalid declared size/);
    await assert.rejects(client.artifact("job-1", artifact(8, "text/html")), /not supported/);
    assert.equal(requests.length, 1, "Only the explicit capabilities request was made");
});

test("a stream exceeding its declared size is cancelled before any excess data is retained", async () => {
    const { response, state } = responseFor([new Uint8Array(4), new Uint8Array(5), new Uint8Array(1)]);
    const { client } = await clientFor(() => response);
    await assert.rejects(client.artifact("job-1", artifact()), /exceeded the declared artifact size/);
    assert.equal(state.cancelled, true); assert.equal(state.reads, 2, "Do not read another chunk after crossing the bound");
});

test("a single oversized stream chunk is cancelled at the 128 MiB budget without a Blob read", async () => {
    const { response, state } = responseFor([new Uint8Array(BROWSER_DOWNLOAD_BUDGET_BYTES + 1)]);
    const { client } = await clientFor(() => response);
    await assert.rejects(client.artifact("job-1", artifact(BROWSER_DOWNLOAD_BUDGET_BYTES)), /128 MiB download budget/);
    assert.equal(state.cancelled, true); assert.equal(state.reads, 1);
});

test("an exact bounded stream produces authenticated safely named bytes and accepts WAV MIME aliases", async () => {
    const { response } = responseFor([new Uint8Array([1, 2, 3]), new Uint8Array([4, 5, 6, 7, 8])], "audio/x-wav; charset=binary");
    const { client, requests } = await clientFor(() => response);
    const result = await client.artifact("job-1", artifact());
    assert.equal(result.name, "server-master.wav"); assert.equal(result.blob.size, 8);
    assert.deepEqual(new Uint8Array(await result.blob.arrayBuffer()), new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]));
    assert.equal(requests.at(-1).init.headers.Authorization, "Bearer private-token");
    assert.equal(requests.at(-1).url.includes("private-token"), false);
});

test("missing, unsafe or mismatched response MIME is cancelled without reading a body chunk", async () => {
    for (const type of ["", "text/html", "application/json", "audio/mpeg"]) {
        const { response, state } = responseFor([new Uint8Array(8)], type);
        const { client } = await clientFor(() => response);
        await assert.rejects(client.artifact("job-1", artifact()), /Content-Type/);
        assert.equal(state.cancelled, true); assert.equal(state.reads, 0);
    }
});

test("truncated streams and unavailable streaming fail without falling back to unbounded response methods", async () => {
    const { response } = responseFor([new Uint8Array(7)]);
    const { client } = await clientFor(() => response);
    await assert.rejects(client.artifact("job-1", artifact()), /Download incomplete/);
    const unavailable = await clientFor(() => new Response(null, { headers: { "Content-Type": "audio/wav" } }));
    await assert.rejects(unavailable.client.artifact("job-1", artifact()), /Bounded download streaming is unavailable/);
});

test("many tiny chunks and a buffer-boundary crossing preserve exact bytes", async () => {
    const bytes = 1024 ** 2 + 17;
    const chunks = [new Uint8Array(1024 ** 2 - 4).fill(42), ...Array.from({ length: 21 }, (_, i) => new Uint8Array([i]))];
    const { response } = responseFor(chunks);
    const { client } = await clientFor(() => response);
    const result = await client.artifact("job-1", artifact(bytes));
    const decoded = new Uint8Array(await result.blob.arrayBuffer());
    assert.equal(decoded.length, bytes);
    assert.equal(decoded[0], 42); assert.equal(decoded[1024 ** 2 - 5], 42);
    assert.deepEqual(decoded.slice(1024 ** 2 - 4), Uint8Array.from({ length: 21 }, (_, i) => i));
});
