class ArrangementCapture extends AudioWorkletProcessor {
    constructor(options) {
        super(); this.values = new Float32Array(8192); this.length = 0; this.window = 0;
        const settings = options.processorOptions || {};
        this.availableFrame = currentFrame + (settings.captureDelayFrames || 0);
        this.notBeforeFrame = 0; this.suppressPcm = settings.suppressPcm === true;
        this.port.onmessage = ({ data }) => {
            if (data.kind !== "measure") return;
            this.window = data.id; this.length = 0;
            this.notBeforeFrame = Math.max(currentFrame, data.notBeforeFrame);
            this.port.postMessage({ kind: "window", id: this.window, first_frame: this.notBeforeFrame });
        };
        this.port.postMessage({ kind: "ready", frame: currentFrame, rate: sampleRate });
    }
    process(inputs, outputs) {
        const channels = inputs[0];
        const frames = outputs[0][0].length;
        for (let frame = 0; frame < frames; frame++) {
            const at = currentFrame + frame;
            if (at < this.availableFrame || at < this.notBeforeFrame) continue;
            if (!this.length) this.firstFrame = at;
            // Missing channels are silence; retain NaN/Infinity for the probe's
            // finite-PCM assertion instead of masking NaN with `|| 0`.
            for (let channel = 0; channel < 2; channel++) this.values[this.length++] = channels[channel]?.[frame] ?? 0;
            if (this.length === this.values.length) {
                this.port.postMessage({ kind: this.suppressPcm ? "suppressed" : "pcm", window: this.window,
                    first_frame: this.firstFrame, end_frame: at + 1, rate: sampleRate, channels: 2 });
                // Keep raw typed-array messages compatible with the original
                // probe, so delayed-arrival reproductions can run that probe.
                if (!this.suppressPcm) {
                    this.port.postMessage(this.values, [this.values.buffer]);
                    this.values = new Float32Array(8192);
                }
                this.length = 0;
            }
        }
        return true;
    }
}
registerProcessor("arrangement-capture", ArrangementCapture);
