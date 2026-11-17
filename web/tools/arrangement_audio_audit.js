// Private qualification instrumentation. It observes the real worker/graph and
// captures a silent side branch; product processing and messages are unchanged.
const audit = window.arrangementAudit = { actions: [], renderBlocks: 0, edits: [], latest: null, preparations: [], proposals: [], measuredOutputs: [], outputs: [], pcm: [], frames: 0 };
audit.deliveries = []; audit.sourceClock = [];
const blobTypes = new Map(), createUrl = URL.createObjectURL;
URL.createObjectURL = function (blob) { const url = createUrl.call(this, blob); blobTypes.set(url, blob.type); return url; };
const anchorClick = HTMLAnchorElement.prototype.click;
HTMLAnchorElement.prototype.click = function (...args) {
    if (this.download) audit.deliveries.push({ name: this.download, type: blobTypes.get(this.href) });
    return anchorClick.apply(this, args);
};
const NativeWorker = window.Worker;
window.Worker = class extends NativeWorker {
    constructor(url, ...args) {
        super(url, ...args);
        if (!new URL(url, location.href).pathname.endsWith("/arrangement-worker.js")) return;
        const requests = new Map(), post = this.postMessage.bind(this);
        this.postMessage = (message, ...rest) => {
            requests.set(message.id, message.action);
            if (message.action !== "render-block") audit.actions.push(message.action);
            else ++audit.renderBlocks;
            if (message.action === "edit") audit.edits.push(structuredClone(message.edits));
            return post(message, ...rest);
        };
        this.addEventListener("message", ({ data }) => {
            const action = requests.get(data.id); requests.delete(data.id);
            if (data.value?.arrangement) audit.latest = data.value;
            if (action === "prepare" && data.value) audit.preparations.push(data.value);
            if (action === "measure-encoded" && data.value) audit.measuredOutputs.push(data.value);
            if (action === "suggest-eq" && data.value) audit.proposals.push(data.value);
        });
    }
};
const connect = AudioNode.prototype.connect;
const NativeWorkletNode = window.AudioWorkletNode;
window.AudioWorkletNode = class extends NativeWorkletNode {
    constructor(context, name, options) {
        super(context, name, options);
        if (name === "arrangement-sink") {
            audit.arrangementContext = context;
            audit.maxBufferedFrames = (options?.processorOptions?.queueBlocks || 3) * 2048;
            this.port.addEventListener("message", ({ data }) => {
                if (!Number.isSafeInteger(data.frame)) return;
                audit.sourceClock.push({ kind: data.kind, generation: data.generation, frame: data.frame, underrun_frames: data.underrun_frames || 0,
                    received_context_time: context.currentTime, received_wall_ms: performance.now() });
                if (audit.sourceClock.length > 512) audit.sourceClock.shift();
            });
            this.port.start();
        }
    }
};
AudioNode.prototype.connect = function (destination, ...args) {
    const result = connect.call(this, destination, ...args);
    if (destination instanceof AudioDestinationNode && this.context === audit.arrangementContext && !audit.outputs.includes(this)) audit.outputs.push(this);
    return result;
};
audit.attach = async () => {
    for (const output of audit.outputs) {
        if (output.arrangementAuditTap) continue;
        const context = output.context;
        await context.audioWorklet.addModule("/__arrangement_capture.js");
        const tap = new AudioWorkletNode(context, "studio-capture", { outputChannelCount: [2] });
        tap.port.onmessage = ({ data }) => {
            if (data.time < audit.after || audit.frozen) return;
            if (!audit.frames) audit.captureStart = data.time;
            if (audit.captureEnd !== null && Math.abs(data.time - audit.captureEnd) > 1 / context.sampleRate) ++audit.captureGaps;
            audit.captureEnd = data.time + data.channels[0].length / context.sampleRate;
            audit.pcm.push(data.channels); audit.frames += data.channels[0].length;
            if (audit.limitFrames && audit.frames >= audit.limitFrames) audit.frozen = true;
            if (audit.pcm.length > 512) audit.pcm.shift();
        };
        connect.call(output, tap); connect.call(tap, context.destination);
        output.arrangementAuditTap = tap; audit.context = context;
    }
};
audit.reset = (limitFrames = 0) => {
    audit.pcm = []; audit.frames = 0; audit.after = (audit.context?.currentTime || 0) + Math.max(.08, (audit.maxBufferedFrames || 0) / (audit.context?.sampleRate || 48000) + .003);
    audit.limitFrames = limitFrames; audit.frozen = false;
    audit.captureStart = audit.captureEnd = null; audit.captureGaps = 0;
    audit.sourceClock = [];
};

// Export the same bounded capture used by measure(), without choosing samples
// by their level. Source-clock receive times are diagnostics, not render times.
audit.captureEvidence = () => ({ measurement: audit.measure(),
    pcm: [0, 1].map(channel => audit.pcm.flatMap(block => Array.from(block[channel] || block[0]))),
    source_clock: audit.sourceClock.slice() });
audit.measure = () => {
    const samples = audit.pcm.flatMap(block => [...block[0]]).slice(-9600);
    if (samples.length !== 9600) throw new Error("Insufficient actual output PCM");
    let peak = 0;
    for (const sample of samples) { if (!Number.isFinite(sample)) throw new Error("Nonfinite actual output PCM"); peak = Math.max(peak, Math.abs(sample)); }
    const tone = hz => {
        let real = 0, imag = 0;
        samples.forEach((sample, index) => { const phase = index * hz * 2 * Math.PI / audit.context.sampleRate;
            real += sample * Math.cos(phase); imag += sample * Math.sin(phase); });
        return Math.hypot(real, imag) * 2 / samples.length;
    };
    return { peak, first: tone(440), second: tone(880), rate: audit.context.sampleRate, clock: audit.context.currentTime,
        max_buffered_frames: audit.maxBufferedFrames, capture_after: audit.after, capture_start: audit.captureStart, capture_end: audit.captureEnd,
        captured_frames: audit.frames, capture_gaps: audit.captureGaps, frozen: audit.frozen,
        measured_start: audit.captureEnd - samples.length / audit.context.sampleRate,
        measured_end: audit.captureEnd };
};
