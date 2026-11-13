export class DspClient {
    constructor(onProgress) {
        this.onProgress = onProgress;
        this.pending = new Map();
        this.sequence = 0;
        this.start();
    }
    start() {
        this.worker = new Worker(new URL("./worker.js", import.meta.url), {
            type: "module",
        });
        this.worker.onmessage = ({ data: m }) => {
            if (m.progress !== undefined) {
                this.onProgress?.(m.progress, m.text);
                return;
            }
            const job = this.pending.get(m.id);
            if (!job) return;
            this.pending.delete(m.id);
            m.error ? job.reject(new Error(m.error)) : job.resolve(m.result);
        };
        this.worker.onerror = (event) => {
            this.fail(
                new Error(
                    event.message ||
                        "Browser DSP worker stopped. Reload your track to retry.",
                ),
            );
        };
    }
    fail(error) {
        for (const job of this.pending.values()) job.reject(error);
        this.pending.clear();
    }
    call(action, data = {}, transfer = []) {
        const id = ++this.sequence;
        return new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject });
            this.worker.postMessage({ id, action, ...data }, transfer);
        });
    }
    cancel() {
        this.worker.terminate();
        this.fail(new DOMException("Processing cancelled", "AbortError"));
        this.start();
    }
}

export function interleave(buffer) {
    const channels = buffer.numberOfChannels,
        output = new Float32Array(buffer.length * channels);
    for (let ch = 0; ch < channels; ch++) {
        const source = buffer.getChannelData(ch);
        for (let i = 0; i < buffer.length; i++)
            output[i * channels + ch] = source[i];
    }
    return output;
}

export function toBuffer(context, samples, channels, rate) {
    const result = context.createBuffer(
        channels,
        samples.length / channels,
        rate,
    );
    for (let ch = 0; ch < channels; ch++) {
        const out = result.getChannelData(ch);
        for (let i = 0; i < out.length; i++)
            out[i] = samples[i * channels + ch];
    }
    return result;
}
