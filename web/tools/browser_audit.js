import { DspClient } from "../dsp-client.js";
import init, { LiveEq, defaults } from "../pkg/dsp.js";

function assert(value, message) {
    if (!value) throw new Error(message);
}
function fixture(seconds = 8, channels = 2, rate = 48000) {
    const pcm = new Float32Array(seconds * rate * channels);
    for (let i = 0; i < seconds * rate; i++) {
        const t = i / rate,
            beat = Math.exp(-(t % 0.5) * 23),
            fade = Math.min(1, t * 10, (seconds - t) * 10);
        for (let ch = 0; ch < channels; ch++)
            pcm[i * channels + ch] =
                fade *
                (0.14 * Math.sin(2 * Math.PI * 75 * t) * beat +
                    0.07 * Math.sin(2 * Math.PI * 440 * t + ch * 0.12) +
                    0.025 * Math.sin(2 * Math.PI * 3100 * t) +
                    0.018 * Math.sin(2 * Math.PI * 11000 * t));
    }
    return pcm;
}
window.auditArtifacts = [];
window.runAudit = async () => {
    const dsp = new DspClient();
    await init();
    const { settings } = JSON.parse(defaults());
    let checks = [];
    try {
        let rejected = false;
        try {
            await dsp.call("export", {
                settings: JSON.stringify(settings),
                format: "wav24",
            });
        } catch {
            rejected = true;
        }
        assert(rejected, "Export must require a master");
        checks.push("export requires master");
        const input = fixture();
        await dsp.call("load", {
            samples: input.slice(),
            rate: 48000,
            channels: 2,
        });
        const measurement = await dsp.call("analyze", {
            settings: JSON.stringify(settings),
        });
        assert(measurement.channels === 2, "Analysis layout");
        const master = await dsp.call("master", {
            settings: JSON.stringify(settings),
        });
        assert(
            master.samples.length === input.length &&
                master.samples.every(Number.isFinite),
            "Master finite and length preserved",
        );
        checks.push("worker master: stereo, finite, duration");
        const suggestion = await dsp.call("suggest", {
                settings: JSON.stringify(settings),
            }),
            again = await dsp.call("suggest", {
                settings: JSON.stringify(settings),
            });
        assert(
            JSON.stringify(suggestion) === JSON.stringify(again),
            "Suggestion must be deterministic",
        );
        assert(
            suggestion.every((b) => Math.abs(b.gain) <= 2.5),
            "Suggestion bounded",
        );
        checks.push("deterministic bounded EQ suggestions");
        settings.manual_eq[2].gain = -3;
        settings.manual_eq[4].gain = 2;
        settings.manual_eq[4].q = 1.4;
        settings.eq_hold_loudness = false;
        const module = await WebAssembly.compile(
            await (await fetch("../pkg/dsp_bg.wasm")).arrayBuffer(),
        );
        const offline = new OfflineAudioContext({
            numberOfChannels: 2,
            length: master.samples.length / 2 + 4096,
            sampleRate: 48000,
        });
        await offline.audioWorklet.addModule("../audio/eq-processor.js");
        const node = new AudioWorkletNode(offline, "slimey-eq", {
            numberOfInputs: 1,
            numberOfOutputs: 1,
            outputChannelCount: [2],
            processorOptions: {
                module,
                channels: 2,
                analysis: master.raw_analysis,
                settings,
            },
        });
        const live = new LiveEq(
            48000,
            2,
            JSON.stringify(master.raw_analysis),
            JSON.stringify(settings),
        );
        let workletError;
        node.onprocessorerror = () => {
            workletError = "processorerror";
        };
        const ready = await new Promise((resolve, reject) => {
            const timeout = setTimeout(
                () =>
                    reject(
                        new Error("Offline worklet initialization timed out"),
                    ),
                15000,
            );
            node.port.onmessage = ({ data }) => {
                if (data.error) {
                    clearTimeout(timeout);
                    reject(new Error(data.error));
                }
                if (data.ready) {
                    clearTimeout(timeout);
                    resolve(data);
                }
            };
        });
        const audio = offline.createBuffer(2, master.samples.length / 2, 48000);
        for (let ch = 0; ch < 2; ch++) {
            const plane = audio.getChannelData(ch);
            for (let i = 0; i < plane.length; i++)
                plane[i] = master.samples[i * 2 + ch];
        }
        const source = offline.createBufferSource();
        source.buffer = audio;
        source.connect(node);
        node.connect(offline.destination);
        source.start();
        // Reproduce exactly the same 128-frame input blocks through the direct WASM API.
        const wasm = await init();
        const expected = new Float32Array(offline.length * 2);
        let blockIn, blockOut, memory;
        for (let frame = 0; frame < offline.length; frame += 128) {
            if (memory !== wasm.memory.buffer) {
                memory = wasm.memory.buffer;
                blockIn = new Float32Array(memory, live.input_ptr(), 4096);
                blockOut = new Float32Array(memory, live.output_ptr(), 4096);
            }
            blockIn.fill(0, 0, 256);
            const count = Math.min(128, offline.length - frame);
            for (let i = 0; i < count; i++)
                for (let ch = 0; ch < 2; ch++)
                    blockIn[i * 2 + ch] =
                        master.samples[(frame + i) * 2 + ch] || 0;
            assert(live.process_block(count), "Direct processor valid");
            expected.set(blockOut.subarray(0, count * 2), frame * 2);
        }
        const output = await offline.startRendering();
        assert(!workletError, "No worklet processor error");
        let maxError = 0,
            peak = 0;
        for (let ch = 0; ch < 2; ch++) {
            const plane = output.getChannelData(ch);
            for (let i = 0; i < plane.length; i++) {
                assert(Number.isFinite(plane[i]), "Finite worklet samples");
                maxError = Math.max(
                    maxError,
                    Math.abs(plane[i] - expected[i * 2 + ch]),
                );
                peak = Math.max(peak, Math.abs(plane[i]));
            }
        }
        assert(
            maxError < 1e-6,
            `Worklet differs from direct shared DSP: ${maxError}`,
        );
        assert(peak < 1, "Live EQ must not digitally clip");
        checks.push("AudioWorklet PCM equals shared Rust processor");
        live.free();
        node.disconnect();
        node.port.close();
        let last;
        for (const format of ["wav16", "wav24", "wav32f"]) {
            last = await dsp.call("export", {
                settings: JSON.stringify(settings),
                format,
            });
            assert(
                last.report.exports[0].decoded_metrics.true_peak_dbtp <=
                    settings.ceiling_dbtp + 0.005,
                "Decoded WAV ceiling",
            );
            window.auditArtifacts.push({
                name: `synthetic-${format}.wav`,
                bytes: last.bytes,
            });
        }
        const repeat = await dsp.call("export", {
            settings: JSON.stringify(settings),
            format: "wav32f",
        });
        assert(
            repeat.bytes.length === last.bytes.length &&
                repeat.bytes.every((v, i) => v === last.bytes[i]),
            "Unchanged exports must be deterministic",
        );
        checks.push(
            "three WAV formats, decoded peak guard, deterministic export",
        );
        await dsp.call("reference", {
            samples: fixture(2, 1),
            rate: 48000,
            channels: 1,
        });
        let staleReferenceExport = false;
        try {
            await dsp.call("export", {
                settings: JSON.stringify(settings),
                format: "wav24",
            });
        } catch {
            staleReferenceExport = true;
        }
        assert(
            staleReferenceExport,
            "Changing reference must invalidate the stored master",
        );
        checks.push("reference changes reject stale worker exports");
        await dsp.call("load", {
            samples: new Float32Array(48000),
            rate: 48000,
            channels: 1,
        });
        const silent = await dsp.call("master", {
            settings: JSON.stringify(settings),
        });
        assert(
            silent.samples.every((v) => v === 0),
            "Silent source must stay silent",
        );
        let noSuggestion = false;
        try {
            await dsp.call("suggest", { settings: JSON.stringify(settings) });
        } catch {
            noSuggestion = true;
        }
        assert(noSuggestion, "Silence must reject EQ suggestions");
        checks.push("mono silence safe; no fabricated suggestion");
        await dsp.call("load", {
            samples: fixture(4, 1),
            rate: 48000,
            channels: 1,
        });
        const mono = await dsp.call("master", {
            settings: JSON.stringify(settings),
        });
        assert(
            mono.samples.length === 192000 &&
                mono.samples.every(Number.isFinite),
            "Mono output",
        );
        checks.push("audible mono master");
        return {
            checks,
            worklet_max_abs_error: maxError,
            worklet_sample_peak: peak,
            worklet_delay: ready.delay,
            master_metrics: master.report.output.metrics,
            success: true,
        };
    } finally {
        dsp.worker.terminate();
    }
};
