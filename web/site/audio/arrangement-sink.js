import { ARRANGEMENT_MAX_QUEUE_BLOCKS } from "./arrangement-buffer.js";

// PCM transport only. Mixing, filters and output protection belong to Rust.
class ArrangementSink extends AudioWorkletProcessor {
    constructor(options = {}) {
        super();
        const requested = options.processorOptions?.queueBlocks;
        this.queueLimit = Number.isInteger(requested) ? Math.max(1, Math.min(ARRANGEMENT_MAX_QUEUE_BLOCKS, requested)) : 3;
        this.underrunFrames = 0; this.starved = false; this.queue = []; this.offset = 0; this.frame = 0; this.generation = 0; this.running = false; this.ended = false;
        this.port.onmessage = ({ data: m }) => {
            if (m.kind === "reset") {
                this.queue = []; this.offset = 0; this.frame = m.frame;
                this.generation = m.generation; this.running = false; this.ended = false; this.underrunFrames = 0; this.starved = false;
                this.port.postMessage({ kind: "reset", generation: this.generation, frame: this.frame });
            } else if (m.generation === this.generation) {
                if (m.kind === "block" && this.queue.length < this.queueLimit) { m.pcm = new Float32Array(m.buffer); this.queue.push(m); }
                if (m.kind === "run") this.running = m.running;
            }
        };
    }
    process(_, outputs) {
        const out = outputs[0];
        for (const channel of out) channel.fill(0);
        if (!this.running) return true;
        let written = 0;
        while (written < out[0].length && this.queue.length) {
            const block = this.queue[0];
            const pcm = block.pcm;
            const count = Math.min(out[0].length - written, block.frames - this.offset);
            for (let ch = 0; ch < out.length; ch++)
                for (let i = 0; i < count; i++) out[ch][written + i] = pcm[(this.offset + i) * block.channels + ch];
            this.offset += count; written += count; this.frame += count;
            if (this.offset === block.frames) {
                this.queue.shift(); this.offset = 0;
                this.port.postMessage({ kind: "consumed", generation: this.generation, frame: this.frame, buffer: block.buffer }, [block.buffer]);
                if (block.final) {
                    this.running = false; this.ended = true;
                    this.port.postMessage({ kind: "ended", generation: this.generation, frame: this.frame }); break;
                }
            }
        }
        // Report actual missing output once per starvation interval. Do not advance
        // source time or skip source samples, and never conceal a dropout by retrying.
        if (this.running && written < out[0].length) {
            this.underrunFrames += out[0].length - written;
            if (!this.starved) this.port.postMessage({ kind: "underrun", generation: this.generation,
                frame: this.frame, underrun_frames: this.underrunFrames });
            this.starved = true;
        } else this.starved = false;
        // Only a small clock message; never project metadata or PCM copies.
        if ((currentFrame & 1023) === 0) this.port.postMessage({ kind: "clock", generation: this.generation, frame: this.frame, underrun_frames: this.underrunFrames });
        return true;
    }
}
registerProcessor("arrangement-sink", ArrangementSink);
