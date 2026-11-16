const modules = new WeakMap();
async function moduleFor(context) {
    if (!context.audioWorklet) throw new Error("Real-time EQ needs AudioWorklet on HTTPS or localhost.");
    if (!modules.has(context)) modules.set(context, (async () => {
        const response = await fetch(new URL("../pkg/dsp_bg.wasm", import.meta.url));
        if (!response.ok) throw new Error("Cannot load the live EQ engine.");
        const module = await WebAssembly.compile(await response.arrayBuffer());
        await context.audioWorklet.addModule(new URL("./eq-processor.js", import.meta.url));
        return module;
    })().catch(error => { modules.delete(context); throw error; }));
    return modules.get(context);
}
export async function createLiveEqNode(context, channels, analysis, settings, { onDelta = () => {}, onError = () => {}, signal } = {}) {
    const module = await moduleFor(context);
    signal?.throwIfAborted();
    const node = new AudioWorkletNode(context, "slimey-eq", { numberOfInputs: 1, numberOfOutputs: 1,
        outputChannelCount: [channels], processorOptions: { module, channels, analysis, settings } });
    let delay;
    try {
        await new Promise((resolve, reject) => {
            const fail = error => { clearTimeout(timeout); reject(error); };
            const timeout = setTimeout(() => fail(new Error("Live EQ could not start in this browser.")), 10000);
            node.onprocessorerror = () => { const error = new Error("The live EQ worklet stopped."); fail(error); onError(error); };
            node.port.onmessage = ({ data }) => {
                if (data.error) { const error = new Error(data.error); fail(error); onError(error); }
                if (data.ready) { clearTimeout(timeout); delay = data.delay / context.sampleRate; resolve(); }
                if (data.delta !== undefined) onDelta(data.delta);
            };
            signal?.addEventListener("abort", () => fail(signal.reason), { once: true });
        });
        signal?.throwIfAborted();
        return { node, delay };
    } catch (error) { node.port.postMessage({ stop: true }); node.port.close(); node.disconnect(); throw error; }
}
