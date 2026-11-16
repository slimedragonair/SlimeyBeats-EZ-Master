import { ArtifactDownloadError } from "./artifact-download.js";

// Match dsp/import_limits.rs and the browser's live-EQ/saved-audio rate range.
const MAX_SECONDS = 15 * 60;
const MAX_PCM_BYTES = 384 * 1024 * 1024;
const MAX_METADATA_BYTES = 64 * 1024;
const UPLOAD_SEGMENT_BYTES = 1024 * 1024;
const FLOAT_GUID = [3, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113];
const UPLOAD_NAME = "browser-track.wav";

function fail(message) { throw new ArtifactDownloadError(message); }

function pcmBytes(frames, rate, channels) {
    if (channels !== 1 && channels !== 2)
        fail("Audio transfer requires mono or stereo audio.");
    if (!Number.isSafeInteger(rate) || rate < 8000 || rate > 192000)
        fail("Audio transfer requires an integer sample rate from 8000 to 192000 Hz.");
    if (!Number.isSafeInteger(frames) || frames < 1)
        fail("Audio transfer requires a positive, complete frame count.");
    if (frames > MAX_PCM_BYTES / (channels * 4))
        fail("Audio transfer exceeds the 384 MiB decoded PCM limit.");
    if (frames / rate > MAX_SECONDS)
        fail("Audio transfer exceeds the 15 minute browser limit.");
    return frames * channels * 4;
}

function fourCC(bytes, offset = 0) {
    return String.fromCharCode(...bytes.subarray(offset, offset + 4));
}

/** Snapshot the immutable original AudioBuffer as little-endian float32 WAV. */
export function uploadWav(audioBuffer) {
    const frames = audioBuffer?.length, rate = audioBuffer?.sampleRate;
    const channels = audioBuffer?.numberOfChannels;
    const size = pcmBytes(frames, rate, channels);
    if (typeof audioBuffer.getChannelData !== "function")
        fail("Audio transfer requires an original AudioBuffer.");
    const planes = Array.from({ length: channels }, (_, ch) => audioBuffer.getChannelData(ch));
    if (planes.some((plane) => !(plane instanceof Float32Array) || plane.length !== frames))
        fail("Audio transfer requires aligned Float32 channel data.");

    const header = new Uint8Array(44), view = new DataView(header.buffer);
    for (const [at, id] of [[0, "RIFF"], [8, "WAVE"], [12, "fmt "], [36, "data"]])
        for (let i = 0; i < 4; i++) header[at + i] = id.charCodeAt(i);
    view.setUint32(4, size + 36, true);
    view.setUint32(16, 16, true);
    view.setUint16(20, 3, true);
    view.setUint16(22, channels, true);
    view.setUint32(24, rate, true);
    view.setUint32(28, rate * channels * 4, true);
    view.setUint16(32, channels * 4, true);
    view.setUint16(34, 32, true);
    view.setUint32(40, size, true);

    const parts = [header];
    const segment = new Uint8Array(Math.min(size, UPLOAD_SEGMENT_BYTES));
    const samples = new DataView(segment.buffer);
    const segmentFrames = segment.length / (channels * 4);
    for (let start = 0; start < frames; start += segmentFrames) {
        const end = Math.min(frames, start + segmentFrames);
        let offset = 0;
        for (let frame = start; frame < end; frame++) {
            for (const plane of planes) {
                const sample = plane[frame];
                if (!Number.isFinite(sample)) fail("Original audio contains nonfinite samples.");
                samples.setFloat32(offset, sample, true);
                offset += 4;
            }
        }
        // Blob snapshots this bounded scratch buffer before it is reused. Keep
        // Blob parts, rather than a second whole-track interleaved byte array.
        parts.push(new Blob([segment.subarray(0, offset)]));
    }
    if (typeof File === "function") return new File(parts, UPLOAD_NAME, { type: "audio/wav" });
    const blob = new Blob(parts, { type: "audio/wav" });
    Object.defineProperty(blob, "name", { value: UPLOAD_NAME });
    return blob;
}

// Retain only the current network chunk. Headers use one 40-byte scratch
// buffer; audio goes directly into the final PCM allocation.
class WavStream {
    constructor(reader, size) {
        this.reader = reader;
        this.size = size;
        this.chunk = new Uint8Array(0);
        this.offset = 0;
        this.received = 0;
        this.position = 0;
    }
    async take(maxBytes) {
        while (this.offset === this.chunk.length) {
            const { done, value } = await this.reader.read();
            if (done) return null;
            if (!(value instanceof Uint8Array)) fail("Invalid WAV response stream.");
            if (value.byteLength > this.size - this.received)
                fail("WAV response exceeded its declared size or byte limit.");
            this.received += value.byteLength;
            this.chunk = value;
            this.offset = 0;
        }
        const count = Math.min(maxBytes, this.chunk.length - this.offset);
        const bytes = this.chunk.subarray(this.offset, this.offset + count);
        this.offset += count;
        this.position += count;
        return bytes;
    }
    async exact(target) {
        for (let offset = 0; offset < target.length;) {
            const bytes = await this.take(target.length - offset);
            if (!bytes) fail("Truncated WAV response.");
            target.set(bytes, offset);
            offset += bytes.length;
        }
    }
    async skip(size) {
        while (size > 0) {
            const bytes = await this.take(size);
            if (!bytes) fail("Truncated WAV chunk.");
            size -= bytes.length;
        }
    }
}

async function readFormat(stream, scratch, size, { rate, channels }) {
    if (size < 16 || size === 17) fail("Malformed WAV format chunk.");
    const prefix = Math.min(size, scratch.length);
    await stream.exact(scratch.subarray(0, prefix));
    const view = new DataView(scratch.buffer), tag = view.getUint16(0, true);
    if (view.getUint16(2, true) !== channels || view.getUint32(4, true) !== rate)
        fail("Master WAV sample rate or channels differ from the original audio.");
    if (view.getUint16(14, true) !== 32 || view.getUint16(12, true) !== channels * 4 ||
        view.getUint32(8, true) !== rate * channels * 4)
        fail("Master WAV requires uncompressed IEEE float32 audio with valid byte alignment.");
    if (size >= 18 && view.getUint16(16, true) !== size - 18)
        fail("Malformed WAV format extension length.");
    if (tag === 65534) {
        if (size < 40 || view.getUint16(16, true) < 22 || view.getUint16(18, true) !== 32 ||
            !FLOAT_GUID.every((byte, i) => scratch[24 + i] === byte))
            fail("Master WAV requires the extensible IEEE float32 GUID and 32 valid bits.");
        const mask = view.getUint32(20, true);
        const speakerCount = mask.toString(2).replaceAll("0", "").length;
        if ((mask & ~0x3ffff) !== 0 || (mask !== 0 && speakerCount !== channels))
            fail("Malformed WAV extensible channel mask.");
    } else if (tag !== 3) {
        fail("Master WAV requires IEEE float32 format 3 or extensible float32.");
    }
    await stream.skip(size - prefix);
}

async function readPcm(stream, output) {
    const bytes = new Uint8Array(output.buffer), view = new DataView(output.buffer);
    let offset = 0, sampleIndex = 0;
    while (offset < bytes.length) {
        const part = await stream.take(bytes.length - offset);
        if (!part) fail("Truncated master WAV audio data.");
        bytes.set(part, offset);
        offset += part.length;
        // A network chunk may end partway through a float. DataView handles
        // unaligned samples and explicit little-endian decoding on every host.
        for (const complete = Math.floor(offset / 4); sampleIndex < complete; sampleIndex++) {
            const sample = view.getFloat32(sampleIndex * 4, true);
            if (!Number.isFinite(sample)) fail("Master WAV contains nonfinite samples.");
            output[sampleIndex] = sample;
        }
    }
}

function wavMime(value) {
    return typeof value === "string" && value.split(";")[0].trim().toLowerCase() === "audio/wav";
}

/** Stream a bounded native WAV master without resampling or changing frames. */
export async function readMasterWav(response, artifact, expected) {
    let reader;
    try {
        const { frames, rate, channels } = expected || {};
        const dataBytes = pcmBytes(frames, rate, channels);
        const size = artifact?.bytes;
        if (!Number.isSafeInteger(size) || size < dataBytes + 44 || size > dataBytes + MAX_METADATA_BYTES)
            fail("Master WAV declared size is invalid or exceeds the PCM plus 64 KiB metadata limit.");
        if (!wavMime(artifact?.content_type) || !wavMime(response.headers?.get("Content-Type")))
            fail("Master WAV requires audio/wav Content-Type for both artifact and response.");
        const length = response.headers.get("Content-Length");
        if (length !== null && (!/^\d+$/.test(length.trim()) || !Number.isSafeInteger(Number(length)) ||
            Number(length) > dataBytes + MAX_METADATA_BYTES || Number(length) !== size))
            fail("Master WAV Content-Length is invalid or differs from its bounded declared size.");
        if (response.ok === false) fail("Master WAV response was not successful.");
        if (!response.body?.getReader) fail("Bounded WAV response streaming is unavailable.");
        reader = response.body.getReader();
        const stream = new WavStream(reader, size), scratch = new Uint8Array(40);
        const view = new DataView(scratch.buffer);
        await stream.exact(scratch.subarray(0, 12));
        if (fourCC(scratch) !== "RIFF" || fourCC(scratch, 8) !== "WAVE")
            fail("Master audio requires a standard little-endian RIFF/WAVE file.");
        if (view.getUint32(4, true) + 8 !== size)
            fail("Master WAV RIFF length differs from its declared size.");

        let formatSeen = false, factSeen = false, output;
        while (stream.position < size) {
            if (size - stream.position < 8) fail("Truncated WAV chunk header.");
            if (!output && stream.position + 8 > MAX_METADATA_BYTES)
                fail("Master WAV header exceeds the 64 KiB limit.");
            await stream.exact(scratch.subarray(0, 8));
            const id = fourCC(scratch), chunkSize = view.getUint32(4, true);
            const padded = chunkSize + (chunkSize & 1);
            if (padded > size - stream.position) fail("WAV chunk length exceeds the RIFF boundary.");
            if (id === "data") {
                if (output || !formatSeen) fail("Master WAV requires one format chunk before one data chunk.");
                if (chunkSize !== dataBytes) fail("Master WAV frame count differs from the original audio.");
                output = new Float32Array(frames * channels);
                await readPcm(stream, output);
            } else {
                if (!output && padded > MAX_METADATA_BYTES - stream.position)
                    fail("Master WAV header exceeds the 64 KiB limit.");
                if (id === "fmt ") {
                    if (formatSeen || output) fail("Duplicate or misplaced WAV format chunk.");
                    await readFormat(stream, scratch, chunkSize, { rate, channels });
                    formatSeen = true;
                } else if (id === "fact") {
                    if (factSeen || chunkSize < 4) fail("Malformed or duplicate WAV fact chunk.");
                    await stream.exact(scratch.subarray(0, 4));
                    if (view.getUint32(0, true) !== frames)
                        fail("Master WAV fact frame count differs from the original audio.");
                    factSeen = true;
                    await stream.skip(chunkSize - 4);
                } else {
                    await stream.skip(chunkSize);
                }
            }
            if (chunkSize & 1) await stream.skip(1);
        }
        if (!output) fail("Master WAV is missing format or audio data.");
        if (await stream.take(1)) fail("Master WAV contains bytes beyond its RIFF length.");
        return output;
    } catch (error) {
        // A hung or rejected cancel must not hide the original validation error.
        try { (reader ? reader.cancel() : response?.body?.cancel())?.catch(() => {}); }
        catch { /* Best effort; the transfer has already failed closed. */ }
        throw error;
    } finally { reader?.releaseLock(); }
}
