class ArrangementCapture extends AudioWorkletProcessor {
    constructor() { super(); this.values = new Float32Array(8192); this.length = 0; }
    process(inputs) {
        const channels = inputs[0];
        for (let frame = 0; frame < 128; frame++) {
            for (let channel = 0; channel < 2; channel++) this.values[this.length++] = channels[channel]?.[frame] || 0;
            if (this.length === this.values.length) {
                this.port.postMessage(this.values, [this.values.buffer]); this.values = new Float32Array(8192); this.length = 0;
            }
        }
        return true;
    }
}
registerProcessor("arrangement-capture", ArrangementCapture);
