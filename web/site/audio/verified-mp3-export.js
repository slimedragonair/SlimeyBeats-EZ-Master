import { encodeMp3Wav } from "./mp3-export.js";
import { analyzeSavedAudio } from "./saved-audio-analysis.js";
import { waveInfo } from "../session-export-audio.js";

const MAX_PASSES = 5;
const PEAK_TOLERANCE_DB = 0.005;
function check(signal) {
    if (signal?.aborted) throw new DOMException("Export cancelled", "AbortError");
}

// Re-encode the immutable resolved float WAV on each safety pass. Re-encoding
// the previous MP3 would accumulate lossy generations and conceal the recipe.
export async function verifiedMp3(wav, { context, client, ceiling, profile = "neutral", signal, onProgress }) {
    const layout = waveInfo(wav);
    if (!Number.isFinite(ceiling) || ceiling > 0 || ceiling < -12)
        throw new Error("Invalid MP3 peak ceiling");
    let gainDb = 0;
    for (let pass = 1; pass <= MAX_PASSES; pass++) {
        check(signal);
        const encoded = await encodeMp3Wav(wav, {
            bitrateKbps: 320, gain_db: gainDb, signal,
            onProgress: (value) => onProgress?.(value * 0.8, `Encoding local MP3 · pass ${pass}…`),
        });
        check(signal);
        onProgress?.(0.85, "Decoding and checking the saved MP3…");
        const result = await analyzeSavedAudio(encoded.bytes, { context, client, profile, signal });
        const duration = layout.frames / layout.sample_rate;
        if (result.channels !== layout.channels || result.sample_rate !== layout.sample_rate ||
            result.duration_seconds < duration - 1 / layout.sample_rate || result.duration_seconds > duration + 0.25)
            throw new Error("Decoded MP3 dimensions differ from the prepared audio. Try WAV export.");
        const peak = result.metrics.true_peak_dbtp;
        if (!Number.isFinite(peak)) throw new Error("Saved MP3 peak could not be measured");
        if (peak <= ceiling + PEAK_TOLERANCE_DB) {
            onProgress?.(1, "Saved MP3 verified");
            return { bytes: encoded.bytes, analysis: result.analysis, metadata: {
                format: "mp3", bitrate_kbps: 320, sample_rate: result.sample_rate,
                decoded_metrics: result.metrics, decoded_frames: result.frames,
                source_frames: layout.frames, encode_passes: pass, safety_gain_db: gainDb,
                dither: false, encoder: encoded.encoder || "LAME 3.100",
            } };
        }
        gainDb += ceiling - peak - 0.15;
    }
    throw new Error("Saved MP3 still exceeds the selected peak ceiling. Try WAV export.");
}
