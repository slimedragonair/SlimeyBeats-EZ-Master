export function assert(value, message) { if (!value) throw new Error(message); }
export function equal(a, b, label) {
    assert(a.length === b.length, `${label}: length changed`);
    for (let i = 0; i < a.length; i++) assert(Object.is(a[i], b[i]), `${label}: sample/byte ${i} differs`);
}
export async function rejects(action, label) {
    let rejected = false;
    try { await action(); } catch { rejected = true; }
    assert(rejected, `${label}: request unexpectedly succeeded`);
}
export function fixture(channels = 2, clipped = true, seconds = 4) {
    const rate = 48000, frames = Math.round(rate * seconds), samples = new Float32Array(frames * channels);
    for (let frame = 0; frame < frames; frame++)
        for (let channel = 0; channel < channels; channel++)
            samples[frame * channels + channel] = 0.12 * Math.sin(2 * Math.PI * (channel ? 740 : 250) * frame / rate);
    if (clipped) {
        const lobe = [0.25, 0.6, 0.9, 1, 1, 1, 0.9, 0.6, 0.25];
        for (let i = 0; i < lobe.length; i++) samples[(rate + i) * channels] = lobe[i];
    }
    return { samples, rate, channels };
}
export function outside(after, before, repair, rate, channels) {
    assert(after.length === before.length, "Repair changed audio length");
    const first = Math.round(repair.start_seconds * rate) * channels,
        last = Math.round(repair.end_seconds * rate) * channels;
    for (let i = 0; i < after.length; i++) {
        assert(Number.isFinite(after[i]), "Repair produced nonfinite PCM");
        if (i < first || i >= last) assert(Object.is(after[i], before[i]), `Repair changed outside sample ${i}`);
    }
}
export function wave(bytes) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const text = at => String.fromCharCode(...bytes.subarray(at, at + 4));
    assert(text(0) === "RIFF" && text(8) === "WAVE", "Expected WAV");
    for (let at = 12; at + 8 <= bytes.length;) {
        const length = view.getUint32(at + 4, true);
        if (text(at) === "data") return new Float32Array(bytes.buffer.slice(bytes.byteOffset + at + 8, bytes.byteOffset + at + 8 + length));
        at += 8 + length + length % 2;
    }
    throw new Error("Missing WAV PCM");
}
