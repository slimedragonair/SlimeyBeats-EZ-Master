import test from "node:test";
import assert from "node:assert/strict";
import { ServerSession } from "./server-session.js";
import { ServerClient } from "./server-client.js";
import { runServerMaster } from "./master-runner.js";
import { uploadWav } from "./pcm-transfer.js";

const settings = { profile: "neutral:0.7+metal:0.3", mode: "loud", target_lufs: -13,
    ceiling_dbtp: -2, intensity: .6, safety: false };
const audio = { length: 16000, sampleRate: 16000, numberOfChannels: 1,
    getChannelData: () => Float32Array.from({ length: 16000 }, (_,i) => .1*Math.sin(i*.1)) };
async function fixture({ failDelete = false, formats = ["wav24", "wav32f", "mp3"] } = {}) {
    const calls = [], input = uploadWav(audio), output = await input.arrayBuffer();
    let body, remaining = true;
    const caps = { api_version: 1, operations: ["master"], formats,
        profiles: [{ id: "neutral", label: "Neutral" }, { id: "metal", label: "Metal" }],
        limits: { max_upload_bytes: 1000000, chunk_bytes: 10000, max_jobs: 4, retention_seconds: 3600 },
        privacy: { temporary_storage: true, retention_seconds: 3600 } };
    const metadata = { target_lufs: -13, ceiling_dbtp: -2, processing_seconds: .1 };
    const report = JSON.stringify(metadata);
    const job = { id: "test-job", state: "uploading", uploaded_bytes: 0, total_bytes: input.size,
        progress: 0, message: "Ready", expires_at: "2026-12-01T00:00:00Z", artifacts: [] };
    const client = new ServerClient({ pageUrl: "https://slimedragonair.github.io/", fetchImpl: async (url, options) => {
        calls.push([new URL(url).pathname, options.method]);
        if (url.endsWith("capabilities")) return Response.json(caps);
        if (options.method === "DELETE") {
            if (failDelete) return new Response("", { status: 503 });
            remaining = false; return Response.json({ deleted: true });
        }
        if (url.endsWith("/v1/jobs")) { body = JSON.parse(options.body); return Response.json(job); }
        if (options.method === "PUT") { job.uploaded_bytes += options.body.size; return Response.json(job); }
        if (url.endsWith("/run")) {
            job.state = "completed"; job.progress = 1;
            job.artifacts = [{ id: "audio", name: "master.wav", bytes: output.byteLength, content_type: "audio/wav" },
                { id: "report", name: "report.json", bytes: report.length, content_type: "application/json" }];
            return Response.json(job);
        }
        if (url.endsWith("/artifacts/audio")) return new Response(output, { headers: { "Content-Type": "audio/wav" } });
        if (url.endsWith("/artifacts/report")) return new Response(report, { headers: { "Content-Type": "application/json" } });
        throw new Error("Unexpected request");
    } });
    const session = new ServerSession({ client });
    await session.connect("https://slimeybeats-preview.tailf8de2d.ts.net", "guest-token");
    return { session, calls, get body() { return body; }, get remaining() { return remaining; } };
}
test("ordinary Master shares settings/reference and returns editable PCM, then confirms deletion", async () => {
    const f = await fixture(), reference = { testReference: true };
    assert.equal(f.calls.length, 1, "Connecting never submits audio");
    const result = await runServerMaster(f.session, audio, settings, reference);
    assert.deepEqual(f.body, { filename: "browser-track.wav", bytes: 64044, ...settings, format: "wav32f",
        reference_analysis: reference, consent: true });
    assert.deepEqual(result.samples, audio.getChannelData(0));
    assert.equal(result.cleanupPending, false); assert.equal(f.remaining, false); assert.equal(f.session.job, null);
});
test("failed cleanup retains access and the completed PCM; next attempt never uploads over unconfirmed old files", async () => {
    const f = await fixture({ failDelete: true });
    const result = await runServerMaster(f.session, audio, settings, null);
    assert.equal(result.cleanupPending, true); assert.equal(result.samples.length, audio.length);
    const creates = f.calls.filter(([path]) => path.endsWith("/v1/jobs")).length;
    await assert.rejects(runServerMaster(f.session, audio, settings, null), /503/);
    assert.equal(f.calls.filter(([path]) => path.endsWith("/v1/jobs")).length, creates);
});
test("unsupported server and cancelled Master never send audio", async () => {
    const f = await fixture({ formats: ["wav24"] });
    await assert.rejects(runServerMaster(f.session, audio, settings, null), /needs an update/);
    const controller = new AbortController(); controller.abort();
    await assert.rejects(runServerMaster(f.session, audio, settings, null, { signal: controller.signal }), { name: "AbortError" });
    assert.equal(f.calls.length, 1);
});
