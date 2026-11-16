import test from "node:test";
import assert from "node:assert/strict";
import { uploadWav, readMasterWav } from "./pcm-transfer.js";

const MIB = 1024 * 1024;
const FLOAT_GUID = [3, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113];
const PLANES = [new Float32Array([0, -0, 0.25, -1.5, 2 ** -149]),
    new Float32Array([1, -0.125, 2, -(2 ** 20), 3.4028234663852886e38])];

function word(value) {
    const bytes = new Uint8Array(4);
    new DataView(bytes.buffer).setUint32(0, value, true);
    return bytes;
}

function chunk(id, payload) {
    const bytes = new Uint8Array(8 + payload.length + (payload.length & 1));
    bytes.set(Array.from(id, (letter) => letter.charCodeAt(0)));
    new DataView(bytes.buffer).setUint32(4, payload.length, true);
    bytes.set(payload, 8);
    return bytes;
}

function riff(chunks) {
    const bytes = new Uint8Array(12 + chunks.reduce((sum, part) => sum + part.length, 0));
    bytes.set([82, 73, 70, 70]);
    new DataView(bytes.buffer).setUint32(4, bytes.length - 8, true);
    bytes.set([87, 65, 86, 69], 8);
    let offset = 12;
    for (const part of chunks) { bytes.set(part, offset); offset += part.length; }
    return bytes;
}

function fixture({ planes = PLANES, rate = 48000, extensible = false, extended = false,
    before = [], after = [], fact = false } = {}) {
    const channels = planes.length, frames = planes[0].length;
    const format = new Uint8Array(extensible ? 40 : extended ? 18 : 16);
    const view = new DataView(format.buffer);
    view.setUint16(0, extensible ? 65534 : 3, true);
    view.setUint16(2, channels, true);
    view.setUint32(4, rate, true);
    view.setUint32(8, rate * channels * 4, true);
    view.setUint16(12, channels * 4, true);
    view.setUint16(14, 32, true);
    if (extensible) {
        view.setUint16(16, 22, true);
        view.setUint16(18, 32, true);
        view.setUint32(20, channels === 1 ? 4 : 3, true);
        format.set(FLOAT_GUID, 24);
    }
    const samples = new Float32Array(frames * channels);
    const audio = new Uint8Array(samples.byteLength), data = new DataView(audio.buffer);
    for (let frame = 0; frame < frames; frame++) for (let ch = 0; ch < channels; ch++) {
        const at = frame * channels + ch;
        samples[at] = planes[ch][frame];
        data.setFloat32(at * 4, samples[at], true);
    }
    const chunks = [...before, chunk("fmt ", format), ...(fact ? [chunk("fact", word(frames))] : []),
        chunk("data", audio), ...after];
    return { bytes: riff(chunks), expected: { frames, rate, channels }, samples, format, audio };
}

function audioBuffer(planes = PLANES, sampleRate = 48000) {
    return { length: planes[0].length, sampleRate, numberOfChannels: planes.length,
        getChannelData: (ch) => planes[ch] };
}

function responseFor(chunks, { type = "audio/wav", length = null, status = 200, cancel } = {}) {
    const state = { reads: 0, cancellations: 0, released: 0 };
    const body = new ReadableStream({
        pull(controller) {
            const next = chunks[state.reads++];
            if (next === undefined) controller.close();
            else controller.enqueue(next);
        },
        cancel,
    }, { highWaterMark: 0 });
    const getReader = body.getReader.bind(body), cancelBody = body.cancel.bind(body);
    body.cancel = (...args) => { state.cancellations++; return cancelBody(...args); };
    body.getReader = () => {
        const reader = getReader(), cancelReader = reader.cancel.bind(reader), release = reader.releaseLock.bind(reader);
        reader.cancel = (...args) => { state.cancellations++; return cancelReader(...args); };
        reader.releaseLock = () => { state.released++; release(); };
        return reader;
    };
    const headers = { "Content-Type": type };
    if (length !== null) headers["Content-Length"] = length;
    const response = new Response(body, { headers, status });
    for (const method of ["blob", "arrayBuffer", "text", "json"])
        response[method] = () => assert.fail(`Unbounded response.${method} must never be called`);
    return { response, state };
}

function exchange(bytes, options = {}) {
    const { response, state } = responseFor(options.chunks || [bytes], options);
    return { response, state, artifact: { bytes: bytes.length, content_type: "audio/wav", ...options.artifact } };
}

async function rejects(bytes, expected, pattern, options = {}) {
    const { response, state, artifact } = exchange(bytes, options);
    await assert.rejects(readMasterWav(response, artifact, expected), pattern);
    assert.equal(state.cancellations, 1, "Every failure initiates cancellation, including pre-read validation");
    assert.equal(response.body.locked, false);
    return state;
}

test("upload snapshots exact immutable mono/stereo float PCM with a fixed name and MIME", async () => {
    for (const planes of [[PLANES[0]], PLANES]) {
        const originals = planes.map((plane) => plane.slice());
        const expected = fixture({ planes });
        const result = uploadWav(audioBuffer(planes));
        assert.equal(result.name, "browser-track.wav");
        assert.equal(result.type, "audio/wav");
        assert.deepEqual(new Uint8Array(await result.arrayBuffer()), expected.bytes);
        for (let ch = 0; ch < planes.length; ch++) assert.deepEqual(planes[ch], originals[ch]);
        planes[0][0] = 0.75;
        assert.deepEqual(new Uint8Array(await result.arrayBuffer()), expected.bytes, "Blob owns its snapshot");
        planes[0].set(originals[0]);
    }
});

test("upload reuses a bounded scratch segment across boundaries, including a Blob-only environment", async () => {
    const frames = MIB / 8 * 2 + 3;
    const planes = [new Float32Array(frames), new Float32Array(frames)];
    planes[0][0] = 0.125; planes[1][MIB / 8] = -0.75; planes[0][frames - 1] = 2;
    const NativeBlob = globalThis.Blob, NativeFile = globalThis.File, buffers = [];
    let segmentCount = 0;
    try {
        globalThis.File = undefined;
        globalThis.Blob = class extends NativeBlob {
            constructor(parts, options) {
                for (const part of parts) if (ArrayBuffer.isView(part)) {
                    assert.ok(part.byteLength <= MIB, "No whole-track interleaved segment");
                    buffers.push(part.buffer);
                }
                super(parts, options);
                if (parts.length === 1 && ArrayBuffer.isView(parts[0])) segmentCount++;
            }
        };
        const result = uploadWav(audioBuffer(planes));
        assert.equal(result.name, "browser-track.wav");
        assert.equal(result.size, 44 + frames * 8);
        assert.equal(result.type, "audio/wav");
        assert.equal(segmentCount, 3);
        assert.equal(new Set(buffers).size, 2, "Only the header and one reusable sample buffer");
        const response = new Response(result.stream(), { headers: { "Content-Type": "audio/wav" } });
        const samples = await readMasterWav(response, { bytes: result.size, content_type: result.type },
            { frames, channels: 2, rate: 48000 });
        for (let frame = 0; frame < frames; frame++) for (let ch = 0; ch < 2; ch++)
            assert.equal(samples[frame * 2 + ch], planes[ch][frame]);
    } finally { globalThis.Blob = NativeBlob; globalThis.File = NativeFile; }
});

test("upload rejects malformed layout and over-limit duration/PCM before accessing samples", () => {
    const valid = { length: 1, sampleRate: 48000, numberOfChannels: 1,
        getChannelData: () => assert.fail("Invalid layout must fail before reading samples") };
    for (const channels of [0, 3, 1.5, "1", null, NaN])
        assert.throws(() => uploadWav({ ...valid, numberOfChannels: channels }), /mono or stereo/);
    for (const rate of [0, -1, 7999, 192001, Infinity, NaN, 48000.5, "48000"])
        assert.throws(() => uploadWav({ ...valid, sampleRate: rate }), /sample rate/);
    for (const frames of [0, -1, NaN, Infinity, 1.5, "1", Number.MAX_SAFE_INTEGER + 1])
        assert.throws(() => uploadWav({ ...valid, length: frames }), /frame count/);
    assert.throws(() => uploadWav({ ...valid, length: 900 * 48000 + 1 }), /15 minute/);
    assert.throws(() => uploadWav({ ...valid, sampleRate: 192000, length: 384 * MIB / 4 + 1 }), /384 MiB/);
    assert.throws(() => uploadWav(null), /mono or stereo/);
    assert.throws(() => uploadWav({ length: 1, sampleRate: 48000, numberOfChannels: 1 }), /AudioBuffer/);
});

test("exact duration and decoded-byte boundaries are admitted without huge test allocations", () => {
    const stop = new Error("layout accepted");
    for (const [length, sampleRate, numberOfChannels] of [[900 * 8000, 8000, 1], [384 * MIB / 8, 192000, 2]])
        assert.throws(() => uploadWav({ length, sampleRate, numberOfChannels,
            getChannelData() { throw stop; } }), (error) => error === stop);
});

test("upload rejects misaligned channel arrays and all nonfinite samples without changing input", () => {
    for (const plane of [new Float32Array(1), [0, 0], new Float64Array(2)])
        assert.throws(() => uploadWav({ length: 2, sampleRate: 48000, numberOfChannels: 1,
            getChannelData: () => plane }), /aligned Float32/);
    for (const sample of [NaN, Infinity, -Infinity]) {
        const plane = new Float32Array([0.25, sample]);
        assert.throws(() => uploadWav(audioBuffer([plane])), /nonfinite/);
        assert.equal(plane[0], 0.25); assert.ok(Object.is(plane[1], sample));
    }
});

test("every byte split, one-byte chunks and empty chunks preserve standard and extensible float samples", async () => {
    for (const extensible of [false, true]) {
        const f = fixture({ extensible, fact: extensible });
        for (let split = 1; split < f.bytes.length; split++) {
            const { response, artifact, state } = exchange(f.bytes,
                { chunks: [f.bytes.subarray(0, split), f.bytes.subarray(split)], length: String(f.bytes.length) });
            assert.deepEqual(await readMasterWav(response, artifact, f.expected), f.samples);
            assert.equal(state.cancellations, 0); assert.equal(state.released, 1);
        }
        const chunks = Array.from(f.bytes, (byte) => new Uint8Array([byte]));
        chunks.splice(2, 0, new Uint8Array(0));
        const { response, artifact } = exchange(f.bytes, { chunks });
        assert.deepEqual(await readMasterWav(response, artifact, f.expected), f.samples);
    }
});

test("mono/stereo rates, format-3 WAVEFORMATEX, padding and pre/post-data metadata retain exact frame identity", async () => {
    for (const rate of [8000, 44100, 48000, 96000, 192000]) for (const planes of [[PLANES[0]], PLANES]) {
        const f = fixture({ rate, planes, extended: true, fact: true,
            before: [chunk("JUNK", new Uint8Array([1]))], after: [chunk("LIST", new Uint8Array([73, 78, 70, 79, 0]))] });
        const { response, artifact } = exchange(f.bytes, { type: "Audio/Wav; charset=binary" });
        const decoded = await readMasterWav(response, artifact, f.expected);
        assert.deepEqual(decoded, f.samples);
        assert.equal(decoded.length, f.expected.frames * f.expected.channels);
    }
    for (const planes of [[PLANES[0]], PLANES]) {
        const f = fixture({ planes, extensible: true, before: [chunk("JUNK", new Uint8Array(1))] });
        const { response, artifact } = exchange(f.bytes);
        assert.deepEqual(await readMasterWav(response, artifact, f.expected), f.samples);
    }
});

test("rejects any change in expected frame count, rate or channel count instead of trimming or resampling", async () => {
    const f = fixture({ before: [chunk("JUNK", new Uint8Array(8))] });
    for (const expected of [{ ...f.expected, frames: 4 }, { ...f.expected, frames: 6 },
        { ...f.expected, channels: 1 }, { ...f.expected, rate: 44100 }])
        await rejects(f.bytes, expected, /frame count|sample rate or channels/);
    const badFact = fixture({ fact: true });
    new DataView(badFact.bytes.buffer).setUint32(44, 4, true);
    await rejects(badFact.bytes, badFact.expected, /fact frame count/);
});

test("MIME, artifact declarations and Content-Length fail before reading and cancel the response", async () => {
    const f = fixture();
    for (const type of ["", "audio/x-wav", "audio/wave", "audio/mpeg", "application/octet-stream", "text/html"])
        assert.equal((await rejects(f.bytes, f.expected, /audio\/wav Content-Type/, { type })).reads, 0);
    for (const content_type of [undefined, "audio/x-wav", "application/json"])
        assert.equal((await rejects(f.bytes, f.expected, /audio\/wav Content-Type/, { artifact: { content_type } })).reads, 0);
    for (const bytes of [undefined, null, -1, 0, 1.5, NaN, Infinity, "84", Number.MAX_SAFE_INTEGER,
        f.samples.byteLength + 65537, f.samples.byteLength + 43])
        assert.equal((await rejects(f.bytes, f.expected, /declared size/, { artifact: { bytes } })).reads, 0);
    for (const length of ["", "-1", "1.5", "NaN", "Infinity", "9007199254740992", "+84", "1e2",
        String(f.bytes.length - 1), String(f.bytes.length + 1), String(f.samples.byteLength + 65537)])
        assert.equal((await rejects(f.bytes, f.expected, /Content-Length/, { length })).reads, 0);
    assert.equal((await rejects(f.bytes, f.expected, /not successful/, { status: 500 })).reads, 0);
});

test("invalid expected layout cancels before any body reads or PCM allocations", async () => {
    const f = fixture();
    for (const [expected, pattern] of [[undefined, /mono or stereo/], [{ ...f.expected, channels: 3 }, /mono or stereo/],
        [{ ...f.expected, rate: 0 }, /sample rate/], [{ ...f.expected, frames: 0 }, /frame count/],
        [{ ...f.expected, frames: Number.MAX_SAFE_INTEGER + 1 }, /frame count/],
        [{ ...f.expected, frames: 900 * 48000 + 1 }, /15 minute/],
        [{ frames: 384 * MIB / 8 + 1, channels: 2, rate: 192000 }, /384 MiB/]])
        assert.equal((await rejects(f.bytes, expected, pattern)).reads, 0);
});

test("malformed roots, unsupported RIFF variants and mismatched RIFF declarations fail closed", async () => {
    const f = fixture();
    for (const [at, text] of [[0, "RF64"], [0, "RIFX"], [0, "riff"], [8, "AVI "]]) {
        const bytes = f.bytes.slice(); bytes.set(Array.from(text, (s) => s.charCodeAt(0)), at);
        await rejects(bytes, f.expected, /RIFF\/WAVE/);
    }
    for (const size of [0, 0xffffffff, f.bytes.length - 9, f.bytes.length - 7]) {
        const bytes = f.bytes.slice(); new DataView(bytes.buffer).setUint32(4, size, true);
        await rejects(bytes, f.expected, /RIFF length/);
    }
});

test("chunk lengths cannot overflow RIFF, bypass frame identity or hide duplicated/reordered audio", async () => {
    const f = fixture();
    for (const [at, value, pattern] of [[16, 0xffffffff, /chunk length/], [40, 0xffffffff, /chunk length/],
        [40, f.samples.byteLength - 1, /frame count/], [16, 17, /format chunk/]]) {
        const bytes = f.bytes.slice(); new DataView(bytes.buffer).setUint32(at, value, true);
        await rejects(bytes, f.expected, pattern);
    }
    for (const chunks of [
        [chunk("data", f.audio), chunk("fmt ", f.format)],
        [chunk("fmt ", f.format), chunk("fmt ", f.format), chunk("data", f.audio)],
        [chunk("fmt ", f.format), chunk("data", f.audio), chunk("data", f.audio)],
        [chunk("fmt ", f.format), chunk("data", f.audio), chunk("fmt ", f.format)],
    ]) await rejects(riff(chunks), f.expected, /one format chunk|Duplicate or misplaced/);
    await rejects(riff([chunk("JUNK", new Uint8Array(80))]), f.expected, /missing format or audio/);
});

test("invalid format encodings, alignment, extension sizes, GUIDs, valid bits and channel masks are rejected", async () => {
    for (const extensible of [false, true]) {
        const f = fixture({ extensible });
        const cases = [[0, 2, 1], [8, 4, 0], [12, 2, 1], [14, 2, 16]];
        if (extensible) cases.push([16, 2, 21], [16, 2, 23], [18, 2, 24], [20, 4, 4], [20, 4, 0x80000003], [24, 4, 1]);
        for (const [at, width, value] of cases) {
            const format = f.format.slice(), view = new DataView(format.buffer);
            if (width === 2) view.setUint16(at, value, true); else view.setUint32(at, value, true);
            await rejects(riff([chunk("fmt ", format), chunk("data", f.audio)]), f.expected,
                /float32|byte alignment|extension length|channel mask/);
        }
        if (extensible) for (const mask of [0, 3]) {
            const format = f.format.slice(); new DataView(format.buffer).setUint32(20, mask, true);
            const { response, artifact } = exchange(riff([chunk("fmt ", format), chunk("data", f.audio)]));
            assert.deepEqual(await readMasterWav(response, artifact, f.expected), f.samples);
        }
    }
    const f = fixture({ extended: true });
    const format = f.format.slice(); new DataView(format.buffer).setUint16(16, 22, true);
    await rejects(riff([chunk("fmt ", format), chunk("data", f.audio)]), f.expected, /extension length/);
});

test("nonfinite PCM is rejected even when float bytes straddle network chunks", async () => {
    for (const sample of [NaN, Infinity, -Infinity]) {
        const f = fixture(), bytes = f.bytes.slice();
        new DataView(bytes.buffer).setFloat32(48, sample, true);
        const state = await rejects(bytes, f.expected, /nonfinite/,
            { chunks: [bytes.subarray(0, 49), bytes.subarray(49, 51), bytes.subarray(51, 60), bytes.subarray(60)] });
        assert.equal(state.reads, 3, "Stop as soon as a complete invalid sample arrives");
    }
});

test("truncation at every byte, including headers, PCM and metadata, cancels without unbounded fallback", async () => {
    const f = fixture({ extensible: true, fact: true, after: [chunk("LIST", new Uint8Array([73, 78, 70, 79, 1]))] });
    for (let end = 0; end < f.bytes.length; end++)
        await rejects(f.bytes, f.expected, /Truncated/, { chunks: [f.bytes.subarray(0, end)] });
    const bytes = riff([chunk("fmt ", f.format), chunk("data", f.audio), new Uint8Array(3)]);
    await rejects(bytes, f.expected, /chunk header/);
    const missingPad = riff([chunk("fmt ", f.format), chunk("data", f.audio), chunk("JUNK", new Uint8Array(1))]).slice(0, -1);
    new DataView(missingPad.buffer).setUint32(4, missingPad.length - 8, true);
    await rejects(missingPad, f.expected, /chunk length/);
});

test("the combined metadata allowance is exactly 64 KiB, before or after audio", async () => {
    for (const placement of ["before", "after"]) {
        const f = fixture({ [placement]: [chunk("JUNK", new Uint8Array(65536 - 44 - 8))] });
        assert.equal(f.bytes.length, f.samples.byteLength + 65536);
        const { response, artifact } = exchange(f.bytes);
        assert.deepEqual(await readMasterWav(response, artifact, f.expected), f.samples);
        const tooLarge = fixture({ [placement]: [chunk("JUNK", new Uint8Array(65536 - 44 - 8 + 1))] });
        assert.equal((await rejects(tooLarge.bytes, tooLarge.expected, /declared size/)).reads, 0);
    }
    const f = fixture(), bytes = new Uint8Array(65536 + f.samples.byteLength);
    bytes.set(f.bytes.subarray(0, 12));
    new DataView(bytes.buffer).setUint32(4, bytes.length - 8, true);
    bytes.set([74, 85, 78, 75], 12);
    new DataView(bytes.buffer).setUint32(16, 65536 - 20 + 1, true);
    await rejects(bytes, f.expected, /header exceeds/);
});

test("extra actual bytes are rejected immediately despite missing or misleading Content-Length", async () => {
    const f = fixture();
    for (const length of [null, String(f.bytes.length)]) {
        const state = await rejects(f.bytes, f.expected, /exceeded its declared size/,
            { length, chunks: [f.bytes, new Uint8Array(1), new Uint8Array(1)] });
        assert.equal(state.reads, 2);
        assert.equal((await rejects(f.bytes, f.expected, /exceeded its declared size/,
            { length, chunks: [new Uint8Array(f.bytes.length + 1)] })).reads, 1);
    }
});

test("invalid streams and missing streaming support fail without consuming whole response objects", async () => {
    const f = fixture();
    await rejects(f.bytes, f.expected, /Invalid WAV response stream/, { chunks: ["not bytes"] });
    const response = new Response(null, { headers: { "Content-Type": "audio/wav" } });
    await assert.rejects(readMasterWav(response, { bytes: f.bytes.length, content_type: "audio/wav" }, f.expected), /streaming is unavailable/);
    const failure = new Error("network stopped");
    let cancelled = false, released = false;
    const broken = { headers: new Headers({ "Content-Type": "audio/wav" }), body: { getReader: () => ({
        read() { throw failure; }, cancel() { cancelled = true; return Promise.reject(new Error("cancel failed")); },
        releaseLock() { released = true; },
    }) } };
    await assert.rejects(readMasterWav(broken, { bytes: f.bytes.length, content_type: "audio/wav" }, f.expected),
        (error) => error === failure);
    assert.equal(cancelled, true); assert.equal(released, true);
});

test("hung cancellation does not delay the original validation failure", async () => {
    const f = fixture();
    await rejects(f.bytes, f.expected, /Content-Type/, { type: "text/html", cancel: () => new Promise(() => {}) });
});
