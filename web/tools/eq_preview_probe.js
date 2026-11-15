import init, { LiveEq, Session, defaults } from "/pkg/dsp.js";
import { Player } from "/audio/player.js";

function assert(value, message) {
    if (!value) throw new Error(message);
}

function summary(values) {
    const sorted = values.slice().sort((a, b) => a - b);
    const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))];
    return { count: values.length, p50_ms: at(0.5), p95_ms: at(0.95), p99_ms: at(0.99), max_ms: at(1) };
}

function fixture(frames, rate, channels) {
    const input = new Float32Array(frames * channels);
    for (let i = 0; i < frames; i++)
        for (let ch = 0; ch < channels; ch++)
            input[i * channels + ch] = 0.07 * Math.sin(2 * Math.PI * 61 * i / rate + ch * 0.2)
                + 0.035 * Math.sin(2 * Math.PI * 997 * i / rate)
                + 0.015 * Math.sin(2 * Math.PI * 11003 * i / rate);
    return input;
}

async function compare(wasm, module, rate, channels) {
    const frames = Math.ceil(rate * 5 / 128) * 128;
    const input = fixture(frames, rate, channels);
    const settings = JSON.parse(defaults()).settings;
    settings.eq_hold_loudness = false;
    for (const band of settings.manual_eq) {
        band.on = true;
        band.gain = 2;
    }
    settings.manual_eq[2].freq = 20;
    settings.manual_eq[2].q = 18;
    const session = new Session(input.slice(), rate, channels);
    const analysis = JSON.parse(session.analyze(JSON.stringify(settings)));
    session.free();
    const blockAt = (seconds) => Math.ceil(rate * seconds / 128) * 128;
    const change = (frequency, gain = 6) => {
        const next = structuredClone(settings);
        next.manual_eq[2].freq = frequency;
        next.manual_eq[2].gain = gain;
        return next;
    };
    const edits = [{ frame: blockAt(2), settings: [change(22)] }];
    // A single turn can deliver many queued pointer messages, then further edits
    // arrive during preparation/fading. The newest target must eventually win.
    edits.push({ frame: blockAt(2.7), settings: Array.from({ length: 40 }, (_, i) => change(25 + i)) });
    for (let i = 1; i <= 20; i++)
        edits.push({ frame: blockAt(2.7) + i * 128, settings: [change(64 + i * 3, -3)] });
    const final = edits.at(-1).settings.at(-1);
    const live = new LiveEq(rate, channels, JSON.stringify(analysis), JSON.stringify(settings));
    const expected = new Float32Array(input.length);
    const steady = new LiveEq(rate, channels, JSON.stringify(analysis), JSON.stringify(final));
    const settled = new Float32Array(input.length);
    const timings = [], edited = [], settingTimes = [];
    let memory, blockIn, blockOut, steadyIn, steadyOut;
    for (let frame = 0; frame < frames; frame += 128) {
        const edit = edits.find((e) => e.frame === frame);
        if (edit) {
            const start = performance.now();
            live.set_settings(JSON.stringify(edit.settings.at(-1)));
            settingTimes.push(performance.now() - start);
        }
        if (memory !== wasm.memory.buffer) {
            memory = wasm.memory.buffer;
            blockIn = new Float32Array(memory, live.input_ptr(), 4096);
            blockOut = new Float32Array(memory, live.output_ptr(), 4096);
            steadyIn = new Float32Array(memory, steady.input_ptr(), 4096);
            steadyOut = new Float32Array(memory, steady.output_ptr(), 4096);
        }
        const block = input.subarray(frame * channels, (frame + 128) * channels);
        blockIn.set(block);
        steadyIn.set(block);
        const start = performance.now();
        assert(live.process_block(128), "Direct live block must be finite");
        const elapsed = performance.now() - start;
        timings.push(elapsed);
        if (edit) edited.push(elapsed);
        expected.set(blockOut.subarray(0, 128 * channels), frame * channels);
        assert(steady.process_block(128), "Steady block must be finite");
        settled.set(steadyOut.subarray(0, 128 * channels), frame * channels);
    }
    const delay = live.delay();
    live.free();
    steady.free();
    const offline = new OfflineAudioContext(channels, frames, rate);
    await offline.audioWorklet.addModule("/audio/eq-preview-instrumented.js");
    const node = new AudioWorkletNode(offline, "slimey-eq-probe", {
        numberOfInputs: 1, numberOfOutputs: 1, outputChannelCount: [channels],
        processorOptions: { module, channels, analysis, settings, edits },
    });
    const errors = [];
    let finishEdits;
    const editsApplied = new Promise((resolve) => { finishEdits = resolve; });
    node.onprocessorerror = () => errors.push("processorerror");
    await new Promise((resolve, reject) => {
        const timeout = setTimeout(() => reject(new Error("Worklet initialization timed out")), 15000);
        node.port.onmessage = ({ data }) => {
            if (data.probeFrame === edits.at(-1).frame) finishEdits(data.probeApplied);
            if (data.error) {
                errors.push(data.error);
                clearTimeout(timeout);
                reject(new Error(data.error));
            }
            if (data.ready) {
                clearTimeout(timeout);
                resolve();
            }
        };
    });
    const buffer = offline.createBuffer(channels, frames, rate);
    for (let ch = 0; ch < channels; ch++)
        for (let i = 0; i < frames; i++) buffer.getChannelData(ch)[i] = input[i * channels + ch];
    const source = offline.createBufferSource();
    source.buffer = buffer;
    source.connect(node);
    node.connect(offline.destination);
    source.start();
    const output = await offline.startRendering();
    const applied = await Promise.race([
        editsApplied,
        new Promise((_, reject) => setTimeout(() => reject(new Error("Control probe timed out")), 15000)),
    ]);
    let maxError = 0, peak = 0, settledError = 0;
    for (let ch = 0; ch < channels; ch++) {
        const plane = output.getChannelData(ch);
        for (let i = 0; i < frames; i++) {
            const sample = plane[i], index = i * channels + ch;
            assert(Number.isFinite(sample), "Worklet PCM must be finite");
            maxError = Math.max(maxError, Math.abs(sample - expected[index]));
            peak = Math.max(peak, Math.abs(sample));
            if (i >= blockAt(4.5)) settledError = Math.max(settledError, Math.abs(sample - settled[index]));
        }
    }
    node.port.postMessage({ stop: true });
    node.disconnect();
    node.port.close();
    assert(!errors.length, errors.join(", "));
    assert(maxError < 1e-6, `Production worklet/direct WASM error ${maxError}`);
    assert(settledError < 1e-5, `Latest burst target did not settle: ${settledError}`);
    assert(peak < 1, `Sample peak ${peak}`);
    assert(applied === edits.length, `Queued settings were not coalesced: ${applied} applies for ${edits.length} edited blocks`);
    return {
        rate, channels, delay, worklet_max_abs_error: maxError, final_target_max_abs_error: settledError,
        sample_peak: peak, blocks: summary(timings), edited_blocks: summary(edited),
        setting_bursts: summary(settingTimes), quantum_ms: 128000 / rate,
        worklet_settings_applied: applied, input_settings_messages: edits.reduce((n, edit) => n + edit.settings.length, 0),
    };
}

async function routing() {
    const errors = [];
    const player = new Player(() => {}, (error) => errors.push(String(error)));
    const context = player.context(), rate = context.sampleRate;
    // Tap before listening gain; mute the actual destination during automation.
    player.volume = 0;
    player.match = false;
    player.updateGains(true);
    const tone = (frequency, amplitude) => {
        const buffer = context.createBuffer(1, rate * 8, rate);
        const plane = buffer.getChannelData(0);
        for (let i = 0; i < plane.length; i++) plane[i] = amplitude * Math.sin(2 * Math.PI * frequency * i / rate);
        return buffer;
    };
    const original = tone(220, 0.04), master = tone(660, 0.09);
    const settings = JSON.parse(defaults()).settings;
    settings.eq_hold_loudness = false;
    const session = new Session(master.getChannelData(0).slice(), rate, 1);
    const analysis = JSON.parse(session.analyze(JSON.stringify(settings)));
    session.free();
    player.setOriginal(original);
    player.setMaster(master, analysis, settings, -12, -8);
    await context.audioWorklet.addModule("/audio/eq-preview-capture.js");
    const tap = new AudioWorkletNode(context, "eq-preview-capture", { outputChannelCount: [1] });
    const blocks = [];
    tap.port.onmessage = ({ data }) => blocks.push(data);
    player.mix.connect(tap);
    tap.connect(context.destination);
    const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    const magnitude = (captured, frequency) => {
        let re = 0, im = 0, count = 0;
        for (const block of captured)
            for (let i = 0; i < block.channels[0].length; i++) {
                const phase = 2 * Math.PI * frequency * (block.time + i / rate);
                re += block.channels[0][i] * Math.cos(phase);
                im += block.channels[0][i] * Math.sin(phase);
                count++;
            }
        assert(count > rate * 0.1, "Enough settled live PCM must be captured");
        return 2 * Math.hypot(re, im) / count;
    };
    const capture = async (wanted, unwanted, expected) => {
        const after = context.currentTime + 0.16;
        await wait(430);
        const selected = blocks.filter((block) => block.time >= after && block.time < context.currentTime);
        const present = magnitude(selected, wanted), absent = magnitude(selected, unwanted);
        assert(Math.abs(present - expected) < 0.002, `Wanted ${wanted} Hz amplitude ${present}, expected ${expected}`);
        assert(absent < 0.001, `Unselected ${unwanted} Hz leaked: ${absent}`);
        return { wanted_hz: wanted, amplitude: present, unwanted_hz: unwanted, unwanted_amplitude: absent };
    };
    try {
        await player.play();
        const sources = [player.dry, player.wet], worklet = player.worklet;
        const a = await capture(220, 660, 0.04);
        const position = player.position();
        player.select("master");
        const b = await capture(660, 220, 0.09);
        assert(player.position() > position, "A/B must preserve the advancing playhead");
        settings.eq_output_db = -6;
        player.updateSettings(settings, 1);
        const edited = await capture(660, 220, 0.09 * 10 ** (-6 / 20));
        assert(player.dry === sources[0] && player.wet === sources[1] && player.worklet === worklet,
            "A/B and live settings must retain the running source instances");
        player.select("original");
        const again = await capture(220, 660, 0.04);
        assert(!errors.length, errors.join(", "));
        return { original: a, master: b, live_gain_edit: edited, original_after_edit: again,
            running_sources_preserved: true, listening_volume: player.volume };
    } finally {
        player.stop();
        player.mix.disconnect(tap);
        tap.disconnect();
        tap.port.close();
        await context.close();
    }
}

window.runEqPreview = async () => {
    const wasm = await init();
    const module = await WebAssembly.compile(await (await fetch("/pkg/dsp_bg.wasm")).arrayBuffer());
    const cases = [];
    for (const [rate, channels] of [[48000, 2], [44100, 1]])
        cases.push(await compare(wasm, module, rate, channels));
    return { cases, routing: await routing(), success: true };
};
