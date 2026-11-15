// Audio belongs in explicit recovery files/IndexedDB, never interface preferences.
export const RECOVERY_VERSION = 1;
export const MAX_SNAPSHOT_BYTES = 128 * 1024 * 1024;
const MAX_HEADER = 256 * 1024;
const MAGIC = "SBREC001";
export function checkBudget(bytes) {
    if (!Number.isSafeInteger(bytes) || bytes < 0 || bytes > MAX_SNAPSHOT_BYTES)
        throw new Error("Recovery is paused: this project exceeds the 128 MiB recovery budget. Editing limits are unchanged. Export audio to keep your work.");
}
export function audioSize(buffer) {
    return buffer ? buffer.length * buffer.numberOfChannels * 4 : 0;
}
export function audioRecord(buffer) {
    if (!buffer) return null;
    return { rate: buffer.sampleRate, channels: buffer.numberOfChannels, frames: buffer.length,
        data: new Blob(Array.from({ length: buffer.numberOfChannels }, (_, ch) => buffer.getChannelData(ch))) };
}
export function validateAudio(record) {
    if (!record || ![1, 2].includes(record.channels) || !Number.isInteger(record.rate) ||
        record.rate < 8000 || record.rate > 192000 || !Number.isInteger(record.frames) ||
        record.frames < 1 || record.frames > record.rate * 900 || !(record.data instanceof Blob) ||
        record.data.size !== record.frames * record.channels * 4)
        throw new Error("Invalid recovery audio. Current work was kept.");
    checkBudget(record.data.size);
}
export async function audioBuffer(context, record) {
    validateAudio(record);
    const samples = new Float32Array(await record.data.arrayBuffer());
    if (!samples.every(Number.isFinite)) throw new Error("Recovery audio contains invalid samples.");
    const buffer = context.createBuffer(record.channels, record.frames, record.rate);
    for (let ch = 0; ch < record.channels; ch++)
        buffer.copyToChannel(samples.subarray(ch * record.frames, (ch + 1) * record.frames), ch);
    return buffer;
}
function tree(value, blobs, unpack = false) {
    if (!unpack && value instanceof Blob) {
        const index = blobs.push(value) - 1;
        return { recovery_blob: index };
    }
    if (!value || typeof value !== "object") return value;
    if (unpack && Object.keys(value).length === 1 && "recovery_blob" in value) {
        if (!Number.isInteger(value.recovery_blob) || !blobs[value.recovery_blob])
            throw new Error("Invalid recovery attachment.");
        return blobs[value.recovery_blob];
    }
    if (Array.isArray(value)) return value.map((v) => tree(v, blobs, unpack));
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, tree(v, blobs, unpack)]));
}
export function snapshotSize(snapshot) {
    const blobs = [], metadata = tree(snapshot, blobs);
    const bytes = new TextEncoder().encode(JSON.stringify(metadata)).length;
    if (bytes > MAX_HEADER) throw new Error("Recovery metadata is too large. Export audio to keep your work.");
    const total = bytes + blobs.reduce((n, b) => n + b.size, 0);
    checkBudget(total);
    return total;
}
export function recoveryFile(snapshot) {
    snapshotSize(snapshot);
    const blobs = [], value = tree(snapshot, blobs);
    const header = new TextEncoder().encode(JSON.stringify({ value, sizes: blobs.map((b) => b.size) }));
    if (header.length > MAX_HEADER) throw new Error("Recovery header is too large.");
    const length = new Uint8Array(4);
    new DataView(length.buffer).setUint32(0, header.length, true);
    return new Blob([MAGIC, length, header, ...blobs], { type: "application/octet-stream" });
}
export async function readRecoveryFile(file) {
    if (file.size > MAX_SNAPSHOT_BYTES + MAX_HEADER + 12 || file.size < 12)
        throw new Error("This recovery file exceeds the recovery budget or is incomplete.");
    const prefix = await file.slice(0, 12).arrayBuffer();
    if (new TextDecoder().decode(prefix.slice(0, 8)) !== MAGIC) throw new Error("Not a SlimeyBeats recovery file.");
    const length = new DataView(prefix).getUint32(8, true);
    if (length > MAX_HEADER || 12 + length > file.size) throw new Error("Invalid recovery header.");
    const { value, sizes } = JSON.parse(await file.slice(12, 12 + length).text());
    if (!Array.isArray(sizes) || sizes.length > 2048 || sizes.some((n) => !Number.isSafeInteger(n) || n < 0) ||
        sizes.reduce((n, b) => n + b, 12 + length) !== file.size) throw new Error("Invalid recovery attachments.");
    let offset = 12 + length;
    const blobs = sizes.map((size) => { const b = file.slice(offset, offset + size); offset += size; return b; });
    const result = tree(value, blobs, true);
    validateSnapshot(result);
    return result;
}
export function validateSnapshot(value) {
    if (!value || value.version !== RECOVERY_VERSION || (!value.master && !value.studio))
        throw new Error("Unsupported or empty recovery point. Current work was kept.");
    snapshotSize(value);
}
