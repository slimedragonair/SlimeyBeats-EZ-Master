import { verifiedMp3 } from "./audio/verified-mp3-export.js";
import { MP3_RATES, validateMp3Layout } from "./audio/mp3-export-input.js";

export function supportsArrangementMp3(snapshot) {
    return MP3_RATES.includes(snapshot.arrangement.sample_rate) && snapshot.preview.frames > 0 &&
        snapshot.preview.frames / snapshot.arrangement.sample_rate <= 900 &&
        snapshot.preview.frames * snapshot.arrangement.channels * 4 <= 384 * 1024 * 1024;
}

// The immutable Float32 render is encoded by the existing LAME worker and
// decoded/remeasured by shared Rust. No mastering, playback-volume or project
// persistence path participates in this explicit delivery operation.
export async function exportArrangementAudio(client, snapshot, { expectedRevision = snapshot.revision,
    format = "wav24", dither = false, signal, onProgress } = {}) {
    if (!["wav16", "wav24", "wav32f", "mp3"].includes(format)) throw new Error("Choose WAV or MP3 arrangement delivery");
    if (format === "mp3") validateMp3Layout(snapshot.preview.frames, snapshot.arrangement.sample_rate, snapshot.arrangement.channels);
    const bytes = await client.request("export-wav", { expectedRevision, format: format === "mp3" ? "wav32f" : format,
        dither: format === "mp3" ? false : dither }, { signal });
    if (format !== "mp3") return bytes;
    if (signal?.aborted) throw new DOMException("Export cancelled", "AbortError");
    const Offline = globalThis.OfflineAudioContext || globalThis.webkitOfflineAudioContext;
    if (!Offline) throw new Error("This browser cannot verify MP3 delivery; choose WAV");
    const context = new Offline(snapshot.arrangement.channels, 1, snapshot.arrangement.sample_rate);
    const encoded = await verifiedMp3(bytes, { context,
        client: { call: (action, data, transfer) => client.request(action, data, { signal, transfer }) },
        ceiling: snapshot.render_options.ceiling_dbtp, profile: "neutral", signal, onProgress });
    const current = await client.request("read", {}, { signal });
    if (current.epoch !== snapshot.epoch || current.revision !== expectedRevision)
        throw new Error("The arrangement changed during MP3 export. Export the current revision again.");
    return encoded.bytes;
}
