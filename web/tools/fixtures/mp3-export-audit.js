export async function auditMp3Export(base) {
    const { encodeMp3, encodeMp3Wav } = await import(`${base}audio/mp3-export.js`);
    const { MP3_LIMITS, readMp3Pcm, validateMp3Layout } = await import(`${base}audio/mp3-export-input.js`);
    const assert = (test, message) => { if (!test) throw new Error(message); };
    const reject = async (job, text) => {
        try { await job(); } catch (error) {
            assert(String(error).includes(text), `Wrong failure: ${error}`);
            return;
        }
        throw new Error(`Expected rejection: ${text}`);
    };
    const bytes64 = (bytes) => {
        let text = "";
        for (let i = 0; i < bytes.length; i += 8192)
            text += String.fromCharCode(...bytes.subarray(i, i + 8192));
        return btoa(text);
    };
    const load = async (name) => new Uint8Array(await (await fetch(`${base}fixtures/${name}.wav`)).arrayBuffer());
    const cases = [], checks = [], context = typeof AudioContext === "function" ? new AudioContext({ sampleRate: 48000 }) : null;
    const browserDecode = context ? "passed" : "not_run: AudioContext unavailable in this browser build";
    try {
        for (const rate of [32000, 44100, 48000]) for (const channels of [1, 2]) {
            const name = `${rate}-${channels}`, input = await load(name), before = input.slice(), progress = [];
            const options = { onProgress: (value) => progress.push(value) };
            const started = performance.now(), encoded = await encodeMp3Wav(input, options);
            assert(input.every((v, i) => v === before[i]), "WAV input mutated/detached");
            assert(progress.at(-1) === 1 && progress[0] === 0 &&
                progress.every((v, i) => v >= 0 && v <= 1 && (!i || v >= progress[i - 1])), "Invalid encode progress");
            assert(encoded.frames === rate * 3 && encoded.channels === channels && encoded.sampleRate === rate &&
                encoded.bitrateKbps === 192 && encoded.peakVerified === false && encoded.gapless === false,
                "Incorrect encoder metadata");
            assert(encoded.blob.type === "audio/mpeg" && encoded.blob.size === encoded.bytes.length, "Invalid MP3 Blob");
            const decoded = context ? await context.decodeAudioData(encoded.bytes.slice().buffer) : null;
            if (decoded) {
                assert(decoded.numberOfChannels === channels && decoded.duration >= 3 && decoded.duration < 3.1,
                    `Browser decoded wrong MP3 layout/duration: ${name}`);
                for (let ch = 0; ch < channels; ch++)
                    assert(decoded.getChannelData(ch).every(Number.isFinite), "Browser decoded nonfinite MP3");
            }
            cases.push({ name, elapsed_ms: performance.now() - started, frames: encoded.frames,
                rate: encoded.sampleRate, channels, browser_decode_frames: decoded?.length,
                browser_decode_rate: decoded?.sampleRate, bytes: bytes64(encoded.bytes) });
        }
        checks.push("six real worker WAV encodes; monotonic progress; immutable inputs; honest metadata");
        const input = await load("48000-2"), pcm = readMp3Pcm({ kind: "wav32f", bytes: input });
        const samples = new Float32Array(pcm.frames * pcm.channels);
        for (let i = 0; i < pcm.frames; i++) for (let ch = 0; ch < pcm.channels; ch++)
            samples[i * pcm.channels + ch] = pcm.sample(i, ch);
        const source = { samples, sampleRate: 48000, channels: 2 };
        const interleaved = await encodeMp3(source);
        assert(samples.byteLength > 0, "Interleaved input detached");
        const reference = cases.find((v) => v.name === "48000-2");
        assert(bytes64(interleaved.bytes) === reference.bytes, "WAV/PCM output differs");
        const planes = [new Float32Array(pcm.frames), new Float32Array(pcm.frames)];
        for (let ch = 0; ch < 2; ch++) for (let i = 0; i < pcm.frames; i++)
            planes[ch][i] = pcm.sample(i, ch);
        if (context) {
            const buffer = context.createBuffer(2, pcm.frames, 48000);
            for (let ch = 0; ch < 2; ch++) buffer.copyToChannel(planes[ch], ch);
            const audioBuffer = await encodeMp3(buffer);
            assert(bytes64(audioBuffer.bytes) === reference.bytes, "AudioBuffer input differs");
        }
        const planar = await encodeMp3({ samples: planes, channels: 2, sampleRate: 48000 });
        assert(bytes64(planar.bytes) === reference.bytes, "Planar input differs");
        const lowered = await encodeMp3Wav(input, { gain_db: -6, bitrateKbps: 320 });
        cases.push({ name: "attenuated", frames: lowered.frames, rate: 48000, channels: 2, bytes: bytes64(lowered.bytes) });
        const owned = input.slice();
        const consumed = await encodeMp3Wav(owned, { transferInput: true });
        assert(owned.byteLength === 0 && bytes64(consumed.bytes) === reference.bytes, "Transfer ownership failed");
        checks.push("planar/interleaved deterministic parity; gain retry; transferred WAV consumption");
        for (const bitrateKbps of [128, 256]) {
            const encoded = await encodeMp3Wav(input, { bitrateKbps });
            cases.push({ name: `bitrate-${bitrateKbps}`, frames: pcm.frames, rate: 48000, channels: 2,
                bitrate: bitrateKbps * 1000, bytes: bytes64(encoded.bytes) });
        }
        await reject(() => encodeMp3Wav(input, { bitrateKbps: 33 }), "bitrate");
        for (const gain_db of [1, -61, NaN, Infinity])
            await reject(() => encodeMp3Wav(input, { gain_db }), "safety gain");
        for (const channels of [0, 3]) await reject(() => encodeMp3({ ...source, channels }), "mono or stereo");
        for (const sampleRate of [0, 22050, 96000, NaN])
            await reject(() => encodeMp3({ ...source, sampleRate }), "Hz");
        await reject(() => encodeMp3({ ...source, samples: new Float32Array(3) }), "complete");
        await reject(() => encodeMp3({ ...source, samples: new Float32Array() }), "complete");
        for (const sample of [NaN, Infinity, -Infinity, 1.01])
            await reject(() => encodeMp3({ samples: new Float32Array([sample]), channels: 1, sampleRate: 48000 }), "out-of-range");
        await reject(() => encodeMp3({ samples: [new Float32Array(2), new Float32Array(1)], channels: 2, sampleRate: 48000 }), "aligned");
        validateMp3Layout(48000 * 900, 48000, 2);
        await reject(() => validateMp3Layout(48000 * 900 + 1, 48000, 2), "limit");
        await reject(() => validateMp3Layout(MP3_LIMITS.maxPcmBytes / 4 + 1, 48000, 1), "limit");
        for (const name of ["truncated", "integer", "partial-frame", "duplicate-data", "overlong"])
            await reject(async () => encodeMp3Wav(await load(name)), name === "integer" ? "float32" : name === "overlong" ? "Truncated" : name === "duplicate-data" ? "Multiple" : name === "partial-frame" ? "complete" : "invalid");
        const odd = await encodeMp3Wav(await load("unaligned"));
        const extended = await encodeMp3Wav(await load("extensible"));
        assert(bytes64(odd.bytes) === reference.bytes && bytes64(extended.bytes) === reference.bytes, "Legal WAV chunks changed audio");
        checks.push("invalid options/layout/samples/RIFF rejected; exact duration bound; extensible and unaligned float WAV accepted");
        const cancelled = new AbortController(); cancelled.abort();
        await reject(() => encodeMp3Wav(input, { signal: cancelled.signal }), "AbortError");
        const during = new AbortController(); let fraction;
        await reject(() => encodeMp3Wav(input, { signal: during.signal, onProgress(value) {
            if (value > 0 && value < 1) { fraction = value; during.abort(); }
        } }), "AbortError");
        assert(fraction > 0 && fraction < 1, "Cancellation did not interrupt actual encoding");
        const concurrent = encodeMp3Wav(input);
        await reject(() => encodeMp3Wav(input), "already running");
        await concurrent;
        await reject(() => encodeMp3Wav(input, { onProgress() { throw new Error("observer failure"); } }), "observer failure");
        const recovered = await encodeMp3Wav(input);
        assert(bytes64(recovered.bytes) === reference.bytes, "Failed/cancelled encoder affected a new export");
        checks.push("pre-start and active worker cancellation; overlap rejection; callback failure; clean subsequent export");
        const tiny = await encodeMp3({ samples: new Float32Array(1), channels: 1, sampleRate: 44100 });
        const silent = await encodeMp3({ samples: new Float32Array(48000), channels: 1, sampleRate: 48000 });
        cases.push({ name: "one-frame", frames: 1, rate: 44100, channels: 1, bytes: bytes64(tiny.bytes) },
            { name: "silence", frames: 48000, rate: 48000, channels: 1, bytes: bytes64(silent.bytes) });
        checks.push("one-frame and silence exports finish");
        return { cases, checks, browser_decode: browserDecode, audio_buffer_adapter: context ? "passed" : "not_run: AudioContext unavailable",
            cancelled_progress: fraction };
    } finally { await context?.close(); }
}

// A separate opt-in long fixture avoids qualifying mobile memory from a short test.
export async function auditMp3Boundary(base) {
    const { encodeMp3Wav } = await import(`${base}audio/mp3-export.js`);
    const { readMp3Timing, normalizeMp3Timing } = await import(`${base}audio/mp3-gapless.js`);
    const { validateDecoded } = await import(`${base}audio/import-validation.js`);
    const input = new Uint8Array(await (await fetch(`${base}fixtures/boundary.wav`)).arrayBuffer());
    const started = performance.now();
    const encoded = await encodeMp3Wav(input, { bitrateKbps: 320, transferInput: true });
    if (input.byteLength || encoded.frames !== 900 * 48000 || encoded.channels !== 2)
        throw new Error("Exact boundary export/transfer failed");
    const result = { name: "boundary", frames: encoded.frames, rate: 48000, channels: 2, bitrate: 320000,
        encode_ms: performance.now() - started, bytes_length: encoded.bytes.length };
    const timing = readMp3Timing(encoded.bytes.buffer);
    if (timing !== null) throw new Error("Unexpected gapless metadata in pinned encoder");
    result.gapless_metadata = "absent";
    if (typeof AudioContext === "function") {
        const context = new AudioContext({ sampleRate: 48000 });
        try {
            const decoded = await context.decodeAudioData(encoded.bytes.slice().buffer);
            const normalized = normalizeMp3Timing(context, decoded, timing, 900);
            result.decoded_frames = decoded.length;
            result.decoded_seconds = decoded.duration;
            result.browser_padding_frames = decoded.length - encoded.frames;
            result.normalizer_preserved_unknown_layout = normalized === decoded;
            if (decoded.length <= encoded.frames || decoded.duration > 900.1 || normalized !== decoded)
                throw new Error("Unexpected exact boundary decoder/padding behavior");
            try {
                validateDecoded(decoded, { max_seconds: 900, max_decoded_bytes: 384 * 1024 * 1024 });
                throw new Error("Padding unexpectedly admitted beyond the import limit");
            } catch (error) {
                if (!String(error).includes("long. The web preview supports up to")) throw error;
                result.reimport = "rejected: unknown MP3 padding puts decoded duration over 900 seconds";
                result.reimport_error = error.message;
            }
        } finally { await context.close(); }
    } else result.reimport = "not_run: AudioContext unavailable";
    let text = "";
    for (let i = 0; i < encoded.bytes.length; i += 8192)
        text += String.fromCharCode(...encoded.bytes.subarray(i, i + 8192));
    result.bytes = btoa(text);
    return result;
}
