// Test-only output tap. This module is served only by verify_studio_live.py.
class StudioCapture extends AudioWorkletProcessor {
    process(inputs, outputs) {
        // Engines may supply no input channels when the graph is all silent.
        const channels = inputs[0]?.length ? inputs[0] : outputs[0];
        this.port.postMessage({ time: currentTime, channels: channels.map((channel) => channel.slice()) });
        return true;
    }
}
registerProcessor("studio-capture", StudioCapture);
