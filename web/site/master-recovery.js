import { audioSize, audioRecord, audioBuffer } from "./recovery-format.js";
import { interleave, DspClient } from "./dsp-client.js";
export function masterRecoverySize(player, reference, baselineReference) {
    return audioSize(player.original) + audioSize(player.master) + (reference?.size || 0) + (baselineReference?.size || 0);
}
export function captureMasterRecovery(state) {
    if (!state.player.original) return null;
    return { source: audioRecord(state.player.original), preview: audioRecord(state.player.master),
        settings: structuredClone(state.settings), baseline: structuredClone(state.baseline),
        signature: state.signature, reference: state.reference || null, baselineReference: state.baselineReference || null,
        serverMetadata: state.player.master ? structuredClone(state.serverMetadata || null) : null,
        name: state.file?.name || "Recovered mix.wav", format: state.format };
}
export async function prepareMasterRecovery(value, player, current, progress) {
    const client = new DspClient(progress);
    try {
        const initial = await client.call("init");
        const source = await audioBuffer(player.context(), value.source);
        const samples = interleave(source);
        await client.call("load", { samples, rate: source.sampleRate, channels: source.numberOfChannels }, [samples.buffer]);
        async function loadReference(blob) {
            if (!(blob instanceof Blob) || blob.size > 128 * 1048576) throw new Error("Invalid recovery reference.");
            const reference = new File([blob], "Recovered reference", { type: blob.type });
            const buffer = await player.decode(reference), samples = interleave(buffer);
            await client.call("reference", { samples, rate: buffer.sampleRate, channels: buffer.numberOfChannels }, [samples.buffer]);
            return reference;
        }
        const settings = value.settings;
        checkSettings(settings, initial.settings);
        await client.call("response", { settings: JSON.stringify(settings), rate: source.sampleRate });
        let preview = null, result = null;
        if (value.preview) {
            checkSettings(value.baseline, initial.settings);
            if (value.baselineReference) await loadReference(value.baselineReference);
            preview = await audioBuffer(player.context(), value.preview);
            if (value.serverMetadata) {
                const samples = interleave(preview);
                result = await client.call("adopt-master", { samples, settings: JSON.stringify(value.baseline || settings),
                    metadata: JSON.stringify(value.serverMetadata) }, [samples.buffer]);
            } else result = await client.call("master", { settings: JSON.stringify(value.baseline || settings) });
            if (preview.sampleRate !== source.sampleRate || preview.numberOfChannels !== source.numberOfChannels ||
                preview.length !== source.length || result.samples.length !== source.length * source.numberOfChannels)
                throw new Error("Invalid recovery preview dimensions.");
            for (let ch = 0; ch < preview.numberOfChannels; ch++) {
                const pcm = preview.getChannelData(ch);
                for (let i = 0; i < pcm.length; i++)
                    if (pcm[i] !== result.samples[i * preview.numberOfChannels + ch])
                        throw new Error("Recovery preview differs with this engine. Keep the file and use the matching app build.");
            }
        }
        let signature = null;
        if (preview) {
            signature = JSON.parse(value.signature);
            if (!signature || typeof signature !== "object" || Array.isArray(signature)) throw new Error("Invalid recovery preview identity.");
        }
        const reference = value.reference ? await loadReference(value.reference) : null;
        if (!current()) throw new Error("New work arrived while recovery was preparing. Current work was kept.");
        return { client, source, preview, reference, result, signature, settings: structuredClone(settings) };
    } catch (error) { client.stop(); throw error; }
}
function checkSettings(value, expected) {
    if (expected === null) {
        if (value !== null && typeof value !== "string" && !(typeof value === "number" && Number.isFinite(value)))
            throw new Error("Invalid nullable recovery setting.");
    } else if (Array.isArray(expected)) {
        if (!Array.isArray(value) || value.length !== expected.length) throw new Error("Invalid recovery setting array.");
        value.forEach((item, i) => checkSettings(item, expected[i]));
    } else if (typeof expected === "object") {
        if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Invalid recovery settings.");
        for (const key of Object.keys(expected)) checkSettings(value[key], expected[key]);
    } else if (typeof value !== typeof expected || (typeof value === "number" && !Number.isFinite(value)))
        throw new Error("Invalid recovery setting value.");
}
export function restoreMasterFields(settings, format) {
    const $ = (id) => document.getElementById(id);
    // Profile blends are represented in the settings string, not inferred from a display label.
    const parts = settings.profile.split("+").map((part) => part.split(":"));
    $("profile").value = parts[0][0];
    $("blend-profile").value = parts[1]?.[0] || "edm";
    $("blend").value = Math.round(Number(parts[1]?.[1] || 0) * 100);
    $("blend-value").textContent = `${$("blend").value}%`;
    $("custom-target").checked = settings.target_lufs !== null;
    if (settings.target_lufs !== null) $("target").value = settings.target_lufs;
    $("target-label").hidden = !$("custom-target").checked;
    for (const [id, key] of [["ceiling", "ceiling_dbtp"], ["intensity", "intensity"], ["eq-output", "eq_output_db"]]) $(id).value = settings[key];
    $("ceiling-value").textContent = `${settings.ceiling_dbtp.toFixed(1)} dBTP`;
    $("intensity-value").textContent = settings.intensity.toFixed(2);
    $("safety").checked = settings.safety;
    $("hold").checked = settings.eq_hold_loudness;
    $("dither").checked = settings.dither;
    $("gentler").classList.toggle("selected", settings.mode === "streaming_safe");
    $("louder").classList.toggle("selected", settings.mode === "loud");
    if (["wav16", "wav24", "wav32f", "mp3"].includes(format)) $("format").value = format;
}
