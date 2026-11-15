import { MP3_LIMITS, readMp3Pcm, validateMp3Options } from "./mp3-export-input.js";
import { createMp3Encoder } from "./mp3-export-encoder.js";

let busy = false;
self.onmessage = async ({ data: message }) => {
    const { id } = message;
    if (busy) {
        self.postMessage({ id, error: "MP3 worker already has an active export" });
        return;
    }
    busy = true;
    let encoder;
    try {
        if (message.action !== "encode") throw new Error("Unknown MP3 worker action");
        const pcm = readMp3Pcm(message.input), options = validateMp3Options(message.options || {});
        const { frames, channels, sampleRate } = pcm, { bitrateKbps, gain_db } = options;
        const progress = (value, text) => self.postMessage({ id, progress: value, text });
        progress(0, "Loading local MP3 encoder");
        encoder = await createMp3Encoder(sampleRate, channels, bitrateKbps);
        const gain = Math.pow(10, gain_db / 20), parts = [];
        let total = 0;
        const append = (part) => {
            total += part.byteLength;
            if (total > MP3_LIMITS.maxOutputBytes) throw new Error("MP3 output exceeds the 40 MiB limit");
            if (part.length) parts.push(part);
        };
        const planar = Array.from({ length: channels }, () => new Float32Array(MP3_LIMITS.chunkFrames));
        for (let first = 0; first < frames; first += MP3_LIMITS.chunkFrames) {
            const count = Math.min(MP3_LIMITS.chunkFrames, frames - first);
            for (let ch = 0; ch < channels; ch++) for (let i = 0; i < count; i++) {
                const sample = pcm.sample(first + i, ch) * gain;
                if (!Number.isFinite(sample) || Math.abs(sample) > 1)
                    throw new Error("MP3 input contains nonfinite or out-of-range samples; render a peak-safe WAV first");
                planar[ch][i] = sample;
            }
            append(encoder.encode(planar.map((v) => v.subarray(0, count))));
            progress(0.98 * (first + count) / frames, "Encoding MP3 on this device");
            // Bound each work unit and let the worker service its event queue.
            await new Promise((resolve) => setTimeout(resolve, 0));
        }
        append(encoder.finish());
        const bytes = new Uint8Array(total);
        let offset = 0;
        for (const part of parts) { bytes.set(part, offset); offset += part.length; }
        if (!bytes.length) throw new Error("MP3 encoder returned empty output");
        encoder.close(); encoder = null;
        self.postMessage({ id, result: { bytes, frames, channels, sampleRate, bitrateKbps, gain_db,
            encoder: "LAME 3.100 (wasm-media-encoders 0.7.0)",
            peakVerified: false, gapless: false } }, [bytes.buffer]);
    } catch (error) {
        self.postMessage({ id, error: error.message || String(error) });
    } finally {
        encoder?.close();
        busy = false;
    }
};
