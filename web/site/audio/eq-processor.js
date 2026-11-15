import "./utf8.js";
import { initSync, LiveEq } from "../pkg/dsp.js";

class SlimeyEq extends AudioWorkletProcessor {
    constructor({ processorOptions: o }) {
        super();
        this.wasm = initSync({ module: o.module });
        this.eq = new LiveEq(
            sampleRate,
            o.channels,
            JSON.stringify(o.analysis),
            JSON.stringify(o.settings),
        );
        this.channels = o.channels;
        this.refreshViews();
        this.port.onmessage = ({ data }) => {
            if (data.stop) {
                this.stopped = true;
                this.eq.free();
                this.port.close();
                return;
            }
            // Pointer bursts can queue many messages between audio blocks. Only
            // the newest complete snapshot affects the next block's samples.
            this.pendingSettings = data;
        };
        this.port.postMessage({
            ready: true,
            delay: this.eq.delay(),
            delta: this.eq.set_settings(JSON.stringify(o.settings)),
        });
    }
    refreshViews() {
        this.memory = this.wasm.memory.buffer;
        this.input = new Float32Array(this.memory, this.eq.input_ptr(), 4096);
        this.output = new Float32Array(this.memory, this.eq.output_ptr(), 4096);
    }
    process(inputs, outputs) {
        if (this.stopped) return false;
        if (this.pendingSettings) {
            const data = this.pendingSettings;
            this.pendingSettings = null;
            try {
                const delta = this.eq.set_settings(JSON.stringify(data.settings));
                this.port.postMessage({ revision: data.revision, delta });
            } catch (error) {
                this.port.postMessage({ error: String(error) });
                return false;
            }
        }
        const source = inputs[0],
            dest = outputs[0],
            frames = dest[0]?.length || 0;
        if (this.memory !== this.wasm.memory.buffer) this.refreshViews();
        for (let i = 0; i < frames; i++)
            for (let ch = 0; ch < this.channels; ch++)
                this.input[i * this.channels + ch] = source[ch]?.[i] || 0;
        if (!this.eq.process_block(frames)) {
            this.port.postMessage({
                error: "Invalid live EQ output; playback stopped.",
            });
            return false;
        }
        for (let i = 0; i < frames; i++)
            for (let ch = 0; ch < this.channels; ch++)
                dest[ch][i] = this.output[i * this.channels + ch];
        return true;
    }
}
registerProcessor("slimey-eq", SlimeyEq);
