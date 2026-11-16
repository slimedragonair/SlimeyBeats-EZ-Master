export const LOCAL_STUDIO_PCM_BYTES = 128 * 1048576;
const MAX_SECONDS = 15 * 60;

/** Wrap an immutable recovery Blob without decoding or copying its PCM. */
export function pcmRecordWav(record, name = "studio-source.wav") {
    const { rate, channels, frames, data } = record || {};
    if (![1, 2].includes(channels) || !Number.isSafeInteger(rate) || rate < 8000 || rate > 192000 ||
        !Number.isSafeInteger(frames) || frames < 1)
        throw new Error("Invalid local Studio PCM geometry.");
    if (frames > rate * MAX_SECONDS) throw new Error("Local Studio migration exceeds the 15 minute origin limit.");
    if (frames > LOCAL_STUDIO_PCM_BYTES / (channels * 4))
        throw new Error("Local Studio migration exceeds the 128 MiB origin limit.");
    const size = frames * channels * 4;
    if (!(data instanceof Blob) || data.size !== size)
        throw new Error("Local Studio PCM Blob does not match its geometry.");

    // Recovery already owns interleaved Float32 PCM. Compose its existing Blob
    // with a fixed RIFF format-3 header; sample validation belongs to decoding.
    const header = new Uint8Array(44), view = new DataView(header.buffer);
    for (const [at, id] of [[0, "RIFF"], [8, "WAVE"], [12, "fmt "], [36, "data"]])
        for (let index = 0; index < 4; index++) header[at + index] = id.charCodeAt(index);
    view.setUint32(4, size + 36, true);
    view.setUint32(16, 16, true);
    view.setUint16(20, 3, true);
    view.setUint16(22, channels, true);
    view.setUint32(24, rate, true);
    view.setUint32(28, rate * channels * 4, true);
    view.setUint16(32, channels * 4, true);
    view.setUint16(34, 32, true);
    view.setUint32(40, size, true);
    if (typeof File === "function") return new File([header, data], name, { type: "audio/wav" });
    const file = new Blob([header, data], { type: "audio/wav" });
    Object.defineProperty(file, "name", { value: name });
    return file;
}
