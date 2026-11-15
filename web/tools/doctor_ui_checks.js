import { assert, equal } from "./doctor_pcm_checks.js";

// Instrument the production client/transport without replacing their DSP.
export async function installDoctorProbe() {
    const resource = name => {
        const entry = performance.getEntriesByType("resource").find(e => new URL(e.name).pathname.endsWith(`/${name}`));
        assert(entry, `Production app did not load ${name}`); return entry.name;
    };
    const { StudioClient } = await import(resource("studio-client.js")),
        { StudioPlayback } = await import(resource("studio-player.js")),
        { subscribeRecovery } = await import(resource("recovery-bridge.js"));
    const probe = window.doctorUi = { calls: [], replies: [], recoveries: 0 };
    subscribeRecovery(() => ++probe.recoveries);
    const call = StudioClient.prototype.call, setAudio = StudioPlayback.prototype.setAudio;
    StudioClient.prototype.call = async function(action, values = {}, ...rest) {
        probe.client = this; probe.calls.push({ action, values: structuredClone(values) });
        const result = await call.call(this, action, values, ...rest);
        if (action === "doctor-scan" && probe.holdScan) {
            probe.held = true; await new Promise(resolve => { probe.release = resolve; }); probe.held = false;
        }
        probe.replies.push({ action, result: structuredClone(result) }); return result;
    };
    StudioPlayback.prototype.setAudio = function(...args) { probe.playback = this; return setAudio.apply(this, args); };
    probe.snapshot = async () => ({ info: JSON.stringify((await probe.client.call("init")).info),
        preview: (await probe.client.call("working")).bytes,
        recipe: await probe.client.call("recovery-snapshot") });
    probe.remember = async () => { probe.before = await probe.snapshot(); probe.recoveryBefore = probe.recoveries; };
    probe.unchanged = async () => {
        const now = await probe.snapshot();
        assert(now.info === probe.before.info, "Read-only UI scan changed metadata/history");
        equal(now.preview, probe.before.preview, "UI scan PCM");
        assert(JSON.stringify(now.recipe.edits) === JSON.stringify(probe.before.recipe.edits), "Scan entered UI recipe");
        assert(probe.recoveries === probe.recoveryBefore, "Read-only UI scan/navigation changed recovery revision");
        return true;
    };
    probe.mutations = () => probe.calls.filter(c => ["doctor", "mix", "history", "replace"].includes(c.action)).length;
    probe.startCapture = async () => {
        const p = probe.playback, context = p.context(); probe.blocks = [];
        await context.audioWorklet.addModule("/__doctor_tests/studio_capture_processor.js");
        const capture = new AudioWorkletNode(context, "studio-capture", { numberOfInputs: 1,
            numberOfOutputs: 1, outputChannelCount: [p.after.numberOfChannels] });
        capture.port.onmessage = ({ data }) => probe.blocks.push(data);
        const silent = context.createGain(); silent.gain.value = 0;
        p.gain.connect(capture); capture.connect(silent); silent.connect(context.destination);
        probe.capture = capture; probe.silent = silent;
    };
    probe.assertSilent = () => {
        const after = probe.captureSince, blocks = probe.blocks.filter(block => block.time >= after);
        assert(blocks.length >= 2, "No settled live output captured");
        for (const block of blocks) for (const channel of block.channels) for (const sample of channel)
            assert(Math.abs(sample) < 1e-7, "All-muted transport emitted audio");
        return { blocks: blocks.length, samples: blocks.reduce((n, b) => n + b.channels.reduce((n, c) => n + c.length, 0), 0) };
    };
    probe.stopCapture = () => {
        probe.playback.gain.disconnect(probe.capture); probe.capture.disconnect(); probe.silent.disconnect();
        probe.capture.port.onmessage = null;
    };
    return true;
}
