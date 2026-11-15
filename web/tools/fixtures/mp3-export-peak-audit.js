// Uses the candidate's real helper, codec and Rust measurement workers.
const assert = (condition, text) => { if (!condition) throw new Error(text); };
const equal = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
async function rejects(job, text) {
    try { await job(); } catch (error) {
        assert(String(error).includes(text), `Unexpected rejection: ${error}`);
        return;
    }
    throw new Error(`Expected rejection: ${text}`);
}
const base64 = (bytes) => {
    let text = "";
    for (let i = 0; i < bytes.length; i += 8192)
        text += String.fromCharCode(...bytes.subarray(i, i + 8192));
    return btoa(text);
};
function wav(samples, rate = 48000, channels = 2) {
    const bytes = new Uint8Array(44 + samples.byteLength), view = new DataView(bytes.buffer);
    for (const [offset, text] of [[0, "RIFF"], [8, "WAVEfmt "], [36, "data"]])
        for (let i = 0; i < text.length; i++) bytes[offset + i] = text.charCodeAt(i);
    view.setUint32(4, bytes.length - 8, true); view.setUint32(16, 16, true);
    view.setUint16(20, 3, true); view.setUint16(22, channels, true);
    view.setUint32(24, rate, true); view.setUint32(28, rate * channels * 4, true);
    view.setUint16(32, channels * 4, true); view.setUint16(34, 32, true);
    view.setUint32(40, samples.byteLength, true);
    for (let i = 0; i < samples.length; i++) view.setFloat32(44 + i * 4, samples[i], true);
    return bytes;
}

export async function auditMp3PeakRetries(base) {
    if (typeof AudioContext !== "function")
        return { status: "not_run", reason: "AudioContext unavailable in this browser build" };
    const { encodeMp3Wav } = await import(`${base}audio/mp3-export.js`);
    const { verifiedMp3 } = await import(`${base}audio/verified-mp3-export.js`);
    const { analyzeSavedAudio } = await import(`${base}audio/saved-audio-analysis.js`);
    const { DspClient } = await import(`${base}dsp-client.js`);
    const { StudioClient } = await import(`${base}studio-client.js`);
    const context = new AudioContext({ sampleRate: 48000 }), master = new DspClient(), studio = new StudioClient();
    const ceiling = -1, candidates = [];
    const measurePcm = async (samples) => master.call("measure-encoded",
        { samples: samples.slice(), rate: 48000, channels: 2, profile: "neutral" });
    try {
        let selected;
        for (const frequency of [237, 997, 440, 73, 1741, 6117]) {
            const samples = new Float32Array(48000 * 2);
            for (let i = 0; i < 48000; i++) for (let ch = 0; ch < 2; ch++) {
                const tone = 1.3 * Math.sin(2 * Math.PI * frequency * i / 48000 + ch * .31);
                samples[i * 2 + ch] = Math.max(-.9, Math.min(.9, tone));
            }
            const initial = await measurePcm(samples);
            const scale = Math.pow(10, (ceiling - .02 - initial.metrics.true_peak_dbtp) / 20);
            for (let i = 0; i < samples.length; i++) samples[i] *= scale;
            const input = wav(samples), source = await measurePcm(samples);
            const encoded = await encodeMp3Wav(input, { bitrateKbps: 320 });
            const decoded = await analyzeSavedAudio(encoded.bytes, { context, client: master });
            const evidence = { frequency, source_dbtp: source.metrics.true_peak_dbtp,
                decoded_dbtp: decoded.metrics.true_peak_dbtp };
            candidates.push(evidence);
            if (decoded.metrics.true_peak_dbtp > ceiling + .005) {
                selected = { input, source, encoded, decoded, frequency };
                break;
            }
        }
        assert(selected, "Synthetic codec overshoot not established: " + JSON.stringify(candidates));
        const { input, source, encoded, decoded } = selected, original = input.slice();
        assert(source.metrics.true_peak_dbtp <= ceiling, "Fixture was not peak-safe before encoding");
        const results = [];
        for (const [name, client] of [["master", master], ["studio", studio]]) {
            const realPost = Worker.prototype.postMessage, messages = [], measurements = [];
            const observed = { async call(...args) {
                const result = await client.call(...args);
                if (args[0] === "measure-encoded") measurements.push(result);
                return result;
            } };
            // Observe real worker inputs before transfer, without replacing any codec/measurement result.
            Worker.prototype.postMessage = function(message, ...args) {
                if (message.action === "encode" && message.input?.kind === "wav32f")
                    messages.push({ bytes: message.input.bytes.slice(), gain_db: message.options.gain_db });
                return realPost.call(this, message, ...args);
            };
            let verified;
            try { verified = await verifiedMp3(input, { context, client: observed, ceiling }); }
            finally { Worker.prototype.postMessage = realPost; }
            assert(verified.metadata.encode_passes >= 2 && verified.metadata.encode_passes <= 5,
                `${name}: real overshoot did not trigger bounded safety retry`);
            assert(messages.length === verified.metadata.encode_passes && measurements.length === messages.length,
                `${name}: encode/measurement passes disagree`);
            assert(messages.every((message) => equal(message.bytes, original)), `${name}: a retry encoded another generation/source`);
            assert(equal(input, original), `${name}: original WAV mutated or consumed`);
            assert(messages[0].gain_db === 0 && messages.at(-1).gain_db === verified.metadata.safety_gain_db &&
                messages.slice(1).every((message, i) => message.gain_db < messages[i].gain_db), `${name}: invalid cumulative attenuation`);
            assert(measurements[0].metrics.true_peak_dbtp > ceiling + .005, `${name}: first pass did not overshoot`);
            assert(verified.metadata.decoded_metrics.true_peak_dbtp <= ceiling + .005, `${name}: unsafe MP3 accepted`);
            const direct = await encodeMp3Wav(original, { bitrateKbps: 320, gain_db: verified.metadata.safety_gain_db });
            assert(equal(verified.bytes, direct.bytes), `${name}: result is not a fresh original-WAV encode`);
            const independent = await analyzeSavedAudio(verified.bytes, { context, client: master });
            assert(independent.metrics.true_peak_dbtp === verified.metadata.decoded_metrics.true_peak_dbtp,
                `${name}: report does not measure returned MP3 bytes`);
            assert(independent.frames === verified.metadata.decoded_frames && independent.frames > 48000,
                `${name}: decoded MP3 padding was hidden`);
            results.push({ name, passes: messages.length, gains_db: messages.map((message) => message.gain_db),
                peaks_dbtp: measurements.map((measurement) => measurement.metrics.true_peak_dbtp),
                final_peak_dbtp: independent.metrics.true_peak_dbtp, decoded_frames: independent.frames,
                original_wav_unchanged: true, every_retry_used_original: true, exact_fresh_encode_match: true,
                bytes: base64(verified.bytes) });
        }
        const otherContext = new AudioContext({ sampleRate: 44100 });
        try {
            await rejects(() => verifiedMp3(input, { context: otherContext, client: master, ceiling }), "dimensions differ");
        } finally { await otherContext.close(); }
        const decodeAbort = new AbortController(); let afterAbortedDecode = 0;
        await rejects(() => verifiedMp3(input, { ceiling, signal: decodeAbort.signal,
            context: { async decodeAudioData(bytes) {
                const decoded = await context.decodeAudioData(bytes); decodeAbort.abort(); return decoded;
            } },
            client: { call(...args) { afterAbortedDecode++; return master.call(...args); } },
        }), "AbortError");
        assert(afterAbortedDecode === 0, "Aborted decoder result was still measured");
        const measureAbort = new AbortController(); let completedMeasurements = 0;
        await rejects(() => verifiedMp3(input, { context, ceiling, signal: measureAbort.signal,
            client: { async call(...args) {
                const measured = await master.call(...args); completedMeasurements++;
                measureAbort.abort(); return measured;
            } },
        }), "AbortError");
        assert(completedMeasurements === 1, "Measurement cancellation did not exercise real Rust analysis");
        return { status: "passed", ceiling_dbtp: ceiling, fixture_frequency: selected.frequency, candidates,
            source_dbtp: source.metrics.true_peak_dbtp, first_pass_dbtp: decoded.metrics.true_peak_dbtp,
            checks: ["real codec overshoot", "original WAV used on every retry", "fresh encode byte equality",
                "returned bytes remeasured", "padding retained", "sample-rate mismatch rejected",
                "abort after actual browser decode", "abort after actual Rust measurement"],
            source_wav_bytes: base64(original), first_pass_bytes: base64(encoded.bytes), results };
    } finally { master.stop(); studio.stop(); await context.close(); }
}
