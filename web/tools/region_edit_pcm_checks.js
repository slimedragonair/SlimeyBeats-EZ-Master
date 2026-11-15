// Independent fixtures and PCM assertions. Never call the region DSP to form an expectation.
export function assert(ok, message) { if (!ok) throw new Error(message); }
export function equal(actual, expected, label) {
    assert(actual.length === expected.length, `${label}: length ${actual.length} != ${expected.length}`);
    for (let i = 0; i < expected.length; i++)
        assert(Object.is(actual[i], expected[i]), `${label}: sample/byte ${i}: ${actual[i]} != ${expected[i]}`);
}
export function fixture(channels = 2, frames = 8001, rate = 8000, seed = 0) {
    const samples = Float32Array.from({ length: frames * channels }, (_, i) => {
        const frame = Math.floor(i / channels), channel = i % channels;
        return (((frame * (channel ? 43 : 17) + seed * 997) % 4093) - (channel ? 3071 : 1021)) / 32768;
    });
    if (frames > 4) { samples[3 * channels] = -0; samples[4 * channels] = 0; }
    return { samples, rate, channels };
}
export function region(kind, start = 1601 / 8000, end = 4001 / 8000) {
    return { start_seconds: start, end_seconds: end,
        operation: { kind, ...(kind === "gain" ? { db: -6 } : {}) } };
}
export function checkEdit(actual, before, edit, channels, rate, label) {
    const start = Math.floor(edit.start_seconds * rate + .5), end = Math.floor(edit.end_seconds * rate + .5);
    assert(actual.length === before.length, `${label}: duration/layout changed`);
    let changed = 0;
    for (let i = 0; i < actual.length; i++) {
        const frame = Math.floor(i / channels), channel = i % channels, input = before[i];
        const outside = frame < start || frame >= end;
        let expected = input;
        if (!outside) {
            const offset = frame - start, count = end - start;
            switch (edit.operation.kind) {
                case "silence": expected = 0; break;
                case "reverse": expected = before[(end - 1 - offset) * channels + channel]; break;
                case "fade_in": expected = count === 1 || offset === 0 ? 0 : Math.fround(input * offset / (count - 1)); break;
                case "fade_out": expected = count === 1 || offset === count - 1 ? 0 : Math.fround(input * (count - 1 - offset) / (count - 1)); break;
                case "gain": expected = Math.fround(input * 10 ** (edit.operation.db / 20)); break;
                default: throw new Error("Unknown test operation");
            }
        }
        assert(Number.isFinite(actual[i]), `${label}: nonfinite sample ${i}`);
        const exact = outside || ["silence", "reverse"].includes(edit.operation.kind) || expected === 0;
        assert(exact ? Object.is(actual[i], expected) : Math.abs(actual[i] - expected) <= Math.max(1e-10, Math.abs(expected) * 2e-7),
            `${label}: ${outside ? "outside selection" : "edited"} frame ${frame}, channel ${channel}: ${actual[i]} != ${expected}`);
        if (!Object.is(actual[i], input)) changed++;
    }
    const identity = (edit.operation.kind === "gain" && edit.operation.db === 0) ||
        (edit.operation.kind === "reverse" && end - start === 1);
    if (!identity)
        assert(changed > 0, `${label}: operation did not change any samples`);
    return { start_frame: start, end_frame_exclusive: end, channels, changed_samples: changed };
}
export function wave(bytes) {
    const data = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const text = (at, length) => String.fromCharCode(...bytes.subarray(at, at + length));
    assert(text(0, 4) === "RIFF" && text(8, 4) === "WAVE", "Export is not WAV");
    let format, pcm;
    for (let at = 12; at + 8 <= bytes.length;) {
        const size = data.getUint32(at + 4, true), body = at + 8;
        assert(body + size <= bytes.length, "Truncated WAV chunk");
        if (text(at, 4) === "fmt ") {
            format = { encoding: data.getUint16(body, true), channels: data.getUint16(body + 2, true),
                rate: data.getUint32(body + 4, true), bits: data.getUint16(body + 14, true) };
            if (format.encoding === 0xfffe) {
                assert(size >= 40 && data.getUint16(body + 16, true) >= 22, "Invalid extensible WAV header");
                equal(bytes.subarray(body + 24, body + 40), [3, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113], "Float32 WAV subtype GUID");
                format.encoding = 3;
            }
        }
        if (text(at, 4) === "data") pcm = bytes.subarray(body, body + size);
        at = body + size + (size & 1);
    }
    assert(format?.encoding === 3 && format.bits === 32 && pcm, "Expected raw float32 WAV");
    const view = new DataView(pcm.buffer, pcm.byteOffset, pcm.byteLength);
    return { ...format, samples: Float32Array.from({ length: pcm.length / 4 }, (_, i) => view.getFloat32(i * 4, true)) };
}
export function base64(bytes) {
    let binary = "";
    for (let at = 0; at < bytes.length; at += 8192) binary += String.fromCharCode(...bytes.subarray(at, at + 8192));
    return btoa(binary);
}
export async function rejects(call, label) {
    let rejected = false;
    try { await call(); } catch { rejected = true; }
    assert(rejected, `${label}: request unexpectedly succeeded`);
}
export const operations = ["silence", "fade_in", "fade_out", "reverse", "gain"];
export function invalidEdits() {
    const valid = region("silence");
    return [
        { ...valid, start_seconds: -1 }, { ...valid, end_seconds: 2 },
        { ...valid, start_seconds: valid.end_seconds }, { ...valid, start_seconds: .8, end_seconds: .2 },
        region("silence", .2, .2 + .1 / 8000),
        { ...valid, start_seconds: null }, { ...valid, end_seconds: "0.5" },
        { ...valid, operation: { kind: "gain" } }, { ...valid, operation: { kind: "gain", db: 12.001 } },
        { ...valid, operation: { kind: "gain", db: -60.001 } }, { ...valid, operation: { kind: "gain", db: null } },
        { ...valid, operation: { kind: "trim" } }, { ...valid, operation: { kind: "silence", db: 1 } },
        { ...valid, extra: true },
    ];
}
