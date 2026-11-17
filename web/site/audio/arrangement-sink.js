// PCM transport only. Mixing, filters and output protection belong to Rust.
class ArrangementSink extends AudioWorkletProcessor {
    constructor() {
        super(); this.queue = []; this.offset = 0; this.frame = 0; this.generation = 0; this.running = false; this.ended = false;
        this.port.onmessage = ({ data: m }) => {
            if (m.kind === "reset") {
                this.queue = []; this.offset = 0; this.frame = m.frame;
                this.generation = m.generation; this.running = false; this.ended = false;
                this.port.postMessage({ kind: "reset", generation: this.generation, frame: this.frame });
            } else if (m.generation === this.generation) {
                if (m.kind === "block" && this.queue.length < 4) { m.pcm = new Float32Array(m.buffer); this.queue.push(m); }
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
        // Only a small clock message; never project metadata or PCM copies.
        if ((currentFrame & 1023) === 0) this.port.postMessage({ kind: "clock", generation: this.generation, frame: this.frame });
        return true;
    }
}
registerProcessor("arrangement-sink", ArrangementSink);
