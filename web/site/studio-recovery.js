import { audioSize, audioRecord, audioBuffer, checkBudget } from "./recovery-format.js";
const KEYS = ["target", "start", "end", "low", "high", "reduction", "repair-mode", "highpass", "noise", "deess",
    "learn-noise", "noise-start", "noise-end", "gate-on", "gate", "hold", "format"];
export function studioFields() {
    return Object.fromEntries(KEYS.map((key) => {
        const input = document.getElementById(`s-${key}`);
        return [key, input.type === "checkbox" ? input.checked : input.value];
    }));
}
export function restoreStudioFields(fields) {
    for (const key of KEYS) {
        const input = document.getElementById(`s-${key}`), value = fields?.[key];
        if (input.type === "checkbox") input.checked = value === true;
        else if (typeof value === "string" && value.length <= 100) input.value = value;
    }
    for (const key of ["noise", "deess"])
        document.getElementById(`s-${key}`).dispatchEvent(new Event("input"));
}
export function studioRecoverySize(playback) {
    // Conservative reservation includes the retained origin and current preview.
    const bank = [...(playback.stems?.values() || [])].reduce((sum, stem) => sum + audioSize(stem.buffer), 0);
    return audioSize(playback.before) + audioSize(playback.after) + bank;
}
export async function captureStudioRecovery(client, playback, state) {
    if (!state.info.loaded) return null;
    checkBudget(studioRecoverySize(playback));
    const recipe = await client.call("recovery-snapshot");
    return { recipe, preview: audioRecord(playback.after), fields: studioFields(),
        controls: structuredClone(state.controls), pendingMix: state.pendingMix,
        dirty: state.dirty, filename: state.filename, committedMix: state.info.controls };
}
export async function prepareStudioRecovery(Client, value, playback, current) {
    const staged = new Client();
    try {
        await staged.call("init");
        const result = await staged.call("recover", { recipe: value.recipe });
        if (value.pendingMix) await staged.call("recovery-validate-mix", { settings: value.controls });
        const expected = await audioBuffer(playback.context(), value.preview);
        if (expected.sampleRate !== result.info.sample_rate || expected.numberOfChannels !== result.info.channels ||
            expected.length * expected.numberOfChannels !== result.samples.length)
            throw new Error("Recovered Studio preview dimensions do not match. Current work was kept.");
        for (let ch = 0; ch < expected.numberOfChannels; ch++) {
            const samples = expected.getChannelData(ch);
            for (let i = 0; i < samples.length; i++)
                if (samples[i] !== result.samples[i * expected.numberOfChannels + ch])
                    throw new Error("Studio recovery does not match this engine's preview. Keep the file and use the matching app build.");
        }
        if (!current()) throw new Error("New work arrived; recovery was cancelled.");
        await staged.call("commit", { proposal: result.proposal });
        return { client: staged, result };
    } catch (error) { staged.stop(); throw error; }
}
