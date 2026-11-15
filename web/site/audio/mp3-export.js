import { MP3_LIMITS, validateMp3Layout, validateMp3Options } from "./mp3-export-input.js";

let active = false;
const aborted = () => new DOMException("MP3 export cancelled", "AbortError");

function run(input, options, transfer) {
    const encoding = validateMp3Options(options);
    if (options.signal?.aborted) return Promise.reject(aborted());
    if (active) return Promise.reject(new Error("An MP3 export is already running"));
    active = true;
    return new Promise((resolve, reject) => {
        let worker, timer, settled = false;
        const finish = (error, result) => {
            if (settled) return;
            settled = true;
            clearTimeout(timer);
            options.signal?.removeEventListener("abort", cancel);
            worker?.terminate();
            active = false;
            error ? reject(error) : resolve(result);
        };
        const cancel = () => finish(aborted());
        try {
            worker = new Worker(new URL("./mp3-export-worker.js", import.meta.url), { type: "module" });
            options.signal?.addEventListener("abort", cancel, { once: true });
            timer = setTimeout(() => finish(new Error("MP3 export timed out; try a shorter track")), MP3_LIMITS.timeoutMs);
            worker.onerror = (event) => finish(new Error(event.message || "MP3 export worker stopped"));
            worker.onmessageerror = () => finish(new Error("MP3 export worker returned unreadable data"));
            worker.onmessage = ({ data }) => {
                if (settled || data.id !== 1) return;
                try {
                    if (data.error) return finish(new Error(data.error));
                    if (data.progress !== undefined) {
                        options.onProgress?.(data.progress, data.text);
                        return;
                    }
                    const result = data.result;
                    if (!(result?.bytes instanceof Uint8Array) || !result.bytes.length ||
                        result.bytes.length > MP3_LIMITS.maxOutputBytes)
                        throw new Error("MP3 export returned invalid output");
                    result.blob = new Blob([result.bytes], { type: "audio/mpeg" });
                    // Reserve completion for a usable artifact, after encoder flush.
                    options.onProgress?.(1, "MP3 encoded; decoded peak verification is required");
                    finish(null, result);
                } catch (error) { finish(error); }
            };
            worker.postMessage({ id: 1, action: "encode", input, options: encoding }, transfer);
        } catch (error) { finish(error); }
    });
}

/** Encode a disposable or retained IEEE float32 WAV snapshot. No decoding on the UI thread.
 * transferInput:true consumes its ArrayBuffer; retain the original for safety retries.
 * gain_db is cumulative attenuation against this original snapshot, never MP3 audio.
 * onProgress(fraction, text); AbortSignal terminates the dedicated worker immediately.
 * The result is NOT peak-verified. Decode and measure before publishing/downloading.
 */
export async function encodeMp3Wav(bytes, options = {}) {
    if (!(bytes instanceof Uint8Array) || bytes.byteLength > MP3_LIMITS.maxWavBytes)
        throw new Error("MP3 input must be a bounded float32 WAV Uint8Array");
    if (options.signal?.aborted) throw aborted();
    if (active) throw new Error("An MP3 export is already running");
    validateMp3Options(options);
    const owned = options.transferInput && bytes.buffer instanceof ArrayBuffer ? bytes : bytes.slice();
    return run({ kind: "wav32f", bytes: owned }, options, [owned.buffer]);
}

/** Accept an AudioBuffer, planar {samples:Float32Array[],sampleRate,channels},
 * or interleaved {samples:Float32Array,sampleRate,channels}. Input is copied.
 */
export async function encodeMp3(source, options = {}) {
    if (options.signal?.aborted) throw aborted();
    if (active) throw new Error("An MP3 export is already running");
    const audioBuffer = typeof source?.getChannelData === "function";
    const channels = audioBuffer ? source.numberOfChannels : source?.channels;
    const sampleRate = source?.sampleRate;
    if (audioBuffer) validateMp3Layout(source.length, sampleRate, channels);
    validateMp3Options(options);
    const samples = audioBuffer ? Array.from({ length: channels }, (_, ch) => source.getChannelData(ch)) : source?.samples;
    const planar = Array.isArray(samples);
    if (!(planar ? samples.length === channels && samples.every((v) => v instanceof Float32Array && v.length === samples[0].length) : samples instanceof Float32Array))
        throw new Error("MP3 input must contain aligned Float32 PCM");
    validateMp3Layout(planar ? samples[0]?.length : samples.length / channels, sampleRate, channels);
    const owned = planar ? samples.map((v) => v.slice()) : samples.slice();
    return run({ kind: planar ? "planar" : "interleaved", samples: owned, sampleRate, channels }, options,
        planar ? owned.map((v) => v.buffer) : [owned.buffer]);
}
