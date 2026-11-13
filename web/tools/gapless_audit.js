import { readMp3Timing, normalizeMp3Timing } from "../audio/mp3-gapless.js";
import { validateDecoded } from "../audio/import-validation.js";

const assert = (value, message) => { if (!value) throw new Error(message); };

function metadata({ version = 3, mono = false, id3 = false, crc = false, flags = 15 } = {}) {
    const start = id3 ? 20 : 0, bytes = new Uint8Array(512 + start);
    const view = new DataView(bytes.buffer);
    const write = (offset, text) => bytes.set(new TextEncoder().encode(text), offset);
    if (id3) { write(0, "ID3"); bytes[3] = 4; bytes[9] = 10; }
    bytes.set([255, 224 | (version << 3) | 2 | (crc ? 0 : 1), 144, mono ? 192 : 0], start);
    const tag = start + 4 + (version === 3 ? (mono ? 17 : 32) : (mono ? 9 : 17)) + (crc ? 2 : 0);
    write(tag, "Info"); view.setUint32(tag + 4, flags); view.setUint32(tag + 8, 100);
    const encoder = tag + 12 + (flags & 2 ? 4 : 0) + (flags & 4 ? 100 : 0) + (flags & 8 ? 4 : 0);
    write(encoder, "LAME3.100");
    const padding = (576 << 12) | 756;
    bytes[encoder + 21] = padding >> 16;
    bytes[encoder + 22] = padding >> 8;
    bytes[encoder + 23] = padding;
    return bytes.buffer;
}

export function runGaplessAudit() {
    const context = new OfflineAudioContext(2, 1, 48000), checks = [];
    for (const options of [{}, { id3: true }, { crc: true }, { flags: 1 }, { version: 2, mono: true }, { version: 0, mono: true }]) {
        const timing = readMp3Timing(metadata(options));
        assert(timing && timing.leadingSamples === 1105, "Parse supported MP3 timing layouts");
        assert(timing.sourceSamples === timing.encodedSamples - 1332, "Declared source samples exclude codec padding");
    }
    for (const encoded of [new ArrayBuffer(0), new Uint8Array([82, 73, 70, 70]).buffer, metadata().slice(0, 80)])
        assert(readMp3Timing(encoded) === null, "Malformed, truncated and non-MP3 headers stay untouched");
    const unknownEncoder = new Uint8Array(metadata());
    unknownEncoder.set(new TextEncoder().encode("XXXX"), 156);
    assert(readMp3Timing(unknownEncoder.buffer) === null, "Do not trust an unknown encoder timing extension");
    const invalidId3 = new Uint8Array(metadata({ id3: true }));
    invalidId3[6] = 128;
    assert(readMp3Timing(invalidId3.buffer) === null, "Reject invalid synchsafe tag lengths");
    checks.push("MP3 timing: MPEG variants, ID3, CRC, optional fields and invalid headers");

    const timing = readMp3Timing(metadata());
    const rawFrames = Math.round(timing.encodedSamples * 48000 / timing.rate);
    const sourceFrames = Math.round(timing.sourceSamples * 48000 / timing.rate);
    const first = Math.round(timing.leadingSamples * 48000 / timing.rate);
    const raw = context.createBuffer(2, rawFrames, 48000);
    for (let ch = 0; ch < 2; ch++) {
        const samples = raw.getChannelData(ch);
        for (let i = 0; i < samples.length; i++) samples[i] = (i % 101) / 101 * (ch ? -1 : 1);
    }
    const corrected = normalizeMp3Timing(context, raw, timing, 900);
    assert(corrected.length === sourceFrames && corrected.numberOfChannels === 2, "Restore declared duration and channels");
    for (let ch = 0; ch < 2; ch++) {
        const actual = corrected.getChannelData(ch), original = raw.getChannelData(ch);
        assert(actual.every((sample, i) => sample === original[first + i]), "Remove only the declared leading/trailing padding");
    }
    assert(normalizeMp3Timing(context, corrected, timing, 900) === corrected, "Do not copy or trim already gapless audio");
    assert(normalizeMp3Timing(context, raw, null, 900) === raw, "Never guess missing timing");
    assert(normalizeMp3Timing(context, raw, timing, 1) === raw, "Never shorten a genuinely overlong source to fit");
    const unknown = context.createBuffer(2, rawFrames - 20, 48000);
    assert(normalizeMp3Timing(context, unknown, timing, 900) === unknown, "Do not trim an unknown or truncated decoder layout");
    checks.push("MP3 padding normalization: exact sample slice, no double trim, unknown/overlong guards");

    let rejected = false;
    try { validateDecoded({ numberOfChannels: 1, duration: 900 + 1 / 48000 }, { max_seconds: 900, max_decoded_bytes: 384 * 1048576 }); }
    catch (error) { rejected = error.message.includes("supports up to 15:00"); }
    assert(rejected, "The strict decoded duration boundary still rejects one extra WAV frame");
    checks.push("WAV duration remains strict; MP3 metadata correction does not widen the cap");
    return checks;
}
