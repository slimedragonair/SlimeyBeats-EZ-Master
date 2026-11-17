import { waveInfo } from "./session-export-audio.js";
import { readMp3Timing, normalizeMp3Timing } from "./audio/mp3-gapless.js";
import { openArrangementArchives, closeArrangementArchives } from "./arrangement-import-archive.js";
import { groupFileVersions } from "./arrangement-import-review.js";

const MAX_BYTES = 128 * 1024 * 1024;
const id = () => crypto.randomUUID();
const check = signal => { if (signal?.aborted) throw new DOMException("Cancelled", "AbortError"); };

export function createArrangementImporter(client, getSnapshot) {
    const reviews = new Map();
    return {
        async reviewImport(files, { signal } = {}) {
            check(signal);
            const opened = await openArrangementArchives(client, files, signal, [...reviews.values()].reduce((sum, value) => sum + value.bytesHeld, 0));
            try {
            check(signal);
            const { parts, entries, skipped_files, notes } = groupFileVersions(opened.inputs);
            const review = { id: id(), parts, skipped_files: skipped_files + opened.skipped, notes };
            reviews.set(review.id, { review, entries, archives: opened.archives, bytesHeld: opened.bytesHeld }); return review;
            } catch (error) { await closeArrangementArchives(client, opened.archives); throw error; }
        },
        async discardImport(review) {
            const value = reviews.get(review?.id);
            reviews.delete(review?.id);
            if (value) await closeArrangementArchives(client, value.archives);
        },
        async addImport(review, selection, { expectedRevision, signal } = {}) {
            check(signal);
            const owned = reviews.get(review?.id), snapshot = getSnapshot();
            if (!owned) throw new Error("This import review expired. Choose the files again.");
            if (expectedRevision !== snapshot.revision) throw new Error("The arrangement changed. Review the current revision before importing.");
            if (!Array.isArray(selection) || !selection.length) throw new Error("Select at least one part");
            const selected = [], used = new Set();
            for (const choice of selection) {
                const part = owned.review.parts.find(part => part.id === choice.part_id);
                const version = part?.versions.find(version => version.id === choice.version_id);
                if (!version || used.has(part.id)) throw new Error("Choose one owned version per part");
                used.add(part.id); selected.push({ part, version, file: owned.entries.get(version.id) });
            }
            const edits = [];
            await client.request("discard-import");
            try {
                for (const { part, version, file } of selected) {
                    check(signal);
                    const bytes = new Uint8Array(await file.arrayBuffer()); check(signal);
                    const asset_id = id(), lane_id = id(), clip_id = id();
                    let frames;
                    if (version.format === "wav") {
                        const layout = waveInfo(bytes); frames = layout.frames;
                        if (layout.sample_rate !== snapshot.arrangement.sample_rate)
                            throw new Error(`Input is ${layout.sample_rate} Hz; this project is ${snapshot.arrangement.sample_rate} Hz. Conform the source explicitly before import.`);
                        await client.request("stage-wav", { asset_id, name: part.name, bytes }, { transfer: [bytes.buffer] });
                    } else {
                        const decoded = await decodeMp3(bytes, snapshot.arrangement.sample_rate);
                        check(signal); frames = decoded.frames;
                        await client.request("stage-asset", { asset_id, name: part.name, samples: decoded.samples,
                            sample_rate: decoded.sample_rate, channels: decoded.channels }, { transfer: [decoded.samples.buffer] });
                    }
                    edits.push({ kind: "add_lane", lane: { id: lane_id, name: part.name, gain_db: 0, pan: 0, mute: false, solo: false,
                        clips: [{ id: clip_id, name: null, source_id: asset_id, start_frame: 0, source_start_frame: 0,
                            end_frame: frames, gain_db: 0, fade_in_frames: 0, fade_out_frames: 0 }] } });
                }
                check(signal);
                const value = await client.request("commit-import", { expectedRevision, edits });
                reviews.delete(review.id); await closeArrangementArchives(client, owned.archives); return value;
            } catch (error) { await client.request("discard-import"); throw error; }
        },
        clear() { reviews.clear(); },
    };
}

function mp3Rate(bytes) {
    let start = 0;
    if (String.fromCharCode(...bytes.subarray(0, 3)) === "ID3" && bytes.length >= 10)
        start = 10 + (bytes[6] * 2097152 + bytes[7] * 16384 + bytes[8] * 128 + bytes[9]);
    for (let i = start; i + 4 < Math.min(bytes.length, start + 65536); i++) {
        if (bytes[i] !== 255 || (bytes[i + 1] & 0xe0) !== 0xe0) continue;
        const version = (bytes[i + 1] >> 3) & 3, layer = (bytes[i + 1] >> 1) & 3, rate = (bytes[i + 2] >> 2) & 3;
        if (version === 1 || layer !== 1 || rate === 3 || (bytes[i + 2] >> 4) === 15 || !(bytes[i + 2] >> 4)) continue;
        return [44100, 48000, 32000][rate] / (version === 3 ? 1 : version === 2 ? 2 : 4);
    }
    throw new Error("Cannot identify this MP3 sample rate");
}
async function decodeMp3(bytes, rate) {
    if (mp3Rate(bytes) !== rate) throw new Error(`MP3 sample rate differs from the ${rate} Hz project. Conform the source explicitly before import.`);
    const Constructor = globalThis.AudioContext || globalThis.webkitAudioContext;
    if (!Constructor) throw new Error("This browser cannot decode MP3 audio");
    const context = new Constructor({ sampleRate: rate });
    try {
        if (context.sampleRate !== rate) throw new Error("The browser cannot decode at this project sample rate");
        const timing = readMp3Timing(bytes.buffer);
        let buffer = await context.decodeAudioData(bytes.buffer);
        buffer = normalizeMp3Timing(context, buffer, timing, 900);
        const frames = buffer.length, channels = buffer.numberOfChannels;
        if (![1, 2].includes(channels) || frames * channels * 4 > MAX_BYTES || frames > rate * 900)
            throw new Error("MP3 exceeds the mono/stereo, 15 minute or 128 MiB decoded input limit");
        const samples = new Float32Array(frames * channels);
        for (let channel = 0; channel < channels; channel++) {
            const values = buffer.getChannelData(channel);
            for (let frame = 0; frame < frames; frame++) samples[frame * channels + channel] = values[frame];
        }
        return { samples, sample_rate: rate, channels, frames };
    } finally { await context.close(); }
}
