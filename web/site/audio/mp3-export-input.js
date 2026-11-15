// MP3 delivery bounds are deliberately independent from general import policy.
export const MP3_LIMITS = Object.freeze({
    maxSeconds: 900,
    maxPcmBytes: 384 * 1024 * 1024,
    maxWavBytes: 385 * 1024 * 1024,
    maxOutputBytes: 40 * 1024 * 1024,
    chunkFrames: 16384,
    timeoutMs: 10 * 60 * 1000,
});
export const MP3_RATES = Object.freeze([32000, 44100, 48000]);
export const MP3_BITRATES = Object.freeze([128, 192, 256, 320]);

export function validateMp3Options(options) {
    const bitrateKbps = options.bitrateKbps ?? 192;
    const gain_db = options.gain_db ?? 0;
    if (!MP3_BITRATES.includes(bitrateKbps))
        throw new Error("MP3 bitrate must be 128, 192, 256 or 320 kbps");
    if (!Number.isFinite(gain_db) || gain_db > 0 || gain_db < -60)
        throw new Error("MP3 safety gain must be between -60 and 0 dB");
    return { bitrateKbps, gain_db };
}

export function validateMp3Layout(frames, sampleRate, channels) {
    if (channels !== 1 && channels !== 2)
        throw new Error("MP3 export supports mono or stereo audio");
    if (!MP3_RATES.includes(sampleRate))
        throw new Error("MP3 export supports 32000, 44100 or 48000 Hz; resample explicitly first");
    if (!Number.isSafeInteger(frames) || frames < 1)
        throw new Error("MP3 input must contain complete, nonempty channel frames");
    if (frames / sampleRate > MP3_LIMITS.maxSeconds || frames * channels * 4 > MP3_LIMITS.maxPcmBytes)
        throw new Error("MP3 input exceeds the 15 minute or 384 MiB PCM limit");
}

export function readMp3Pcm(input) {
    if (input.kind === "wav32f") return readFloatWav(input.bytes);
    const { samples, sampleRate, channels } = input;
    const planar = input.kind === "planar";
    if (planar) {
        if (!Array.isArray(samples) || samples.length !== channels ||
            !samples.every((v) => v instanceof Float32Array && v.length === samples[0].length))
            throw new Error("MP3 planar channels must have equal Float32Array lengths");
    } else if (input.kind !== "interleaved" || !(samples instanceof Float32Array)) {
        throw new Error("MP3 input must contain Float32 PCM");
    }
    const frames = planar ? samples[0]?.length : samples.length / channels;
    validateMp3Layout(frames, sampleRate, channels);
    return { frames, sampleRate, channels,
        sample: planar ? (i, ch) => samples[ch][i] : (i, ch) => samples[i * channels + ch] };
}

function readFloatWav(bytes) {
    if (!(bytes instanceof Uint8Array) || bytes.byteLength < 12 || bytes.byteLength > MP3_LIMITS.maxWavBytes)
        throw new Error("MP3 input must be a bounded float32 RIFF/WAVE file");
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const text = (at, n) => String.fromCharCode(...bytes.subarray(at, at + n));
    if (text(0, 4) !== "RIFF" || text(8, 4) !== "WAVE" || view.getUint32(4, true) + 8 !== bytes.byteLength)
        throw new Error("MP3 input has an invalid or truncated RIFF/WAVE header");
    let format, data;
    for (let offset = 12; offset < bytes.length;) {
        if (offset + 8 > bytes.length) throw new Error("Truncated WAV chunk header");
        const id = text(offset, 4), size = view.getUint32(offset + 4, true), at = offset + 8;
        const end = at + size, next = end + (size & 1);
        if (next > bytes.length) throw new Error("Truncated WAV chunk data");
        if (id === "fmt ") {
            if (format || size < 16) throw new Error("Invalid WAV format chunk");
            const tag = view.getUint16(at, true), channels = view.getUint16(at + 2, true);
            const sampleRate = view.getUint32(at + 4, true), bits = view.getUint16(at + 14, true);
            const floatGuid = [3,0,0,0,0,0,16,0,128,0,0,170,0,56,155,113];
            const extensible = tag === 65534 && size >= 40 && view.getUint16(at + 16, true) >= 22 &&
                view.getUint16(at + 18, true) === 32 && floatGuid.every((v, i) => bytes[at + 24 + i] === v);
            if ((tag !== 3 && !extensible) || bits !== 32 ||
                view.getUint16(at + 12, true) !== channels * 4 ||
                view.getUint32(at + 8, true) !== sampleRate * channels * 4)
                throw new Error("MP3 input requires uncompressed IEEE float32 WAV audio");
            format = { channels, sampleRate };
        } else if (id === "data") {
            if (data) throw new Error("Multiple WAV data chunks are unsupported");
            data = { at, size };
        }
        offset = next;
    }
    if (!format || !data) throw new Error("WAV is missing format or audio data");
    const { channels, sampleRate } = format, frames = data.size / (channels * 4);
    validateMp3Layout(frames, sampleRate, channels);
    // DataView also accepts legal RIFF data chunks that are not four-byte aligned.
    return { frames, sampleRate, channels,
        sample: (i, ch) => view.getFloat32(data.at + (i * channels + ch) * 4, true) };
}
