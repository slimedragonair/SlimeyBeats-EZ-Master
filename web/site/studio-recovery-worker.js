const MAX_EDITS = 128;
export function applyStudioEdit(candidate, action, values) {
    switch (action) {
        case "mix": return candidate.mix(JSON.stringify(values.settings));
        case "voice": return candidate.voice(values.target, JSON.stringify(values.settings));
        case "repair": return candidate.repair(values.target, JSON.stringify(values.selection));
        case "doctor": return candidate.doctor(values.target, JSON.stringify(values.repair));
        case "replace": return candidate.replace_stem(values.kind);
        case "region": return candidate.edit_region(values.kind, JSON.stringify(values.edit));
        case "history": return candidate.history(values.redo);
        default: throw new Error("Unsupported recovery edit.");
    }
}
function edit(action, m) {
    switch (action) {
        case "mix": return { action, settings: m.settings };
        case "voice": return { action, target: m.target, settings: m.settings };
        case "repair": return { action, target: m.target, selection: m.selection };
        case "doctor": return { action, target: m.target, repair: m.repair };
        case "replace": return { action, kind: m.kind };
        case "region": return { action, kind: m.kind, edit: m.edit };
        case "history": return { action, redo: m.redo };
        default: throw new Error("Unsupported Studio edit.");
    }
}
function pcm(studio, kind, info, wasm) {
    const length = studio.stem_len(kind);
    return { rate: info.sample_rate, channels: info.channels, frames: length / info.channels,
        data: new Blob([new Float32Array(wasm.memory.buffer, studio.stem_ptr(kind), length)]) };
}
async function samples(record) {
    if (!record || ![1, 2].includes(record.channels) || !Number.isInteger(record.rate) ||
        !Number.isInteger(record.frames) || record.frames < 1 || record.frames > record.rate * 900 ||
        !(record.data instanceof Blob) || record.data.size !== record.frames * record.channels * 4 || record.data.size > 128 * 1048576)
        throw new Error("Invalid Studio recovery PCM.");
    const result = new Float32Array(await record.data.arrayBuffer());
    if (!result.every(Number.isFinite)) throw new Error("Invalid Studio recovery samples.");
    return result;
}
export class StudioRecoveryRecipe {
    constructor(BrowserStudio) { this.BrowserStudio = BrowserStudio; this.edits = []; }
    commit(action, values, studio) {
        if (action === "recover") return;
        if (["load", "stems"].includes(action)) {
            this.origin?.free(); this.origin = studio.fork(); this.edits = []; this.overflow = false;
        } else if (this.edits.length < MAX_EDITS) this.edits.push(edit(action, values));
        else this.overflow = true;
    }
    snapshot(wasm) {
        if (!this.origin || this.overflow)
            throw new Error("Recovery is paused: Studio exceeds its 128-edit recovery recipe. Editing still works. Export audio or start a new Studio import to keep a recoverable session.");
        const info = JSON.parse(this.origin.info());
        return { source: info.stems.length ? null : pcm(this.origin, "source", info, wasm),
            stems: info.stems.map((kind) => ({ kind, name: info.stem_names?.[kind] ?? kind, audio: pcm(this.origin, kind, info, wasm) })),
            edits: structuredClone(this.edits) };
    }
    async restore(value) {
        if (!value || !Array.isArray(value.stems) || !Array.isArray(value.edits) || value.edits.length > MAX_EDITS || value.stems.length > 1024)
            throw new Error("Invalid Studio recovery recipe.");
        const candidate = new this.BrowserStudio();
        try {
            if (value.stems.length) {
                let total = 0;
                for (const stem of value.stems) {
                    total += stem.audio?.data?.size || 0;
                    if (total > 128 * 1048576) throw new Error("Studio recovery bank exceeds its budget.");
                    candidate.stage_stem(stem.kind, await samples(stem.audio), stem.audio.rate, stem.audio.channels);
                }
                candidate.commit_stems();
                for (const stem of value.stems) candidate.set_stem_display_name(stem.kind, stem.name);
            } else candidate.load(await samples(value.source), value.source.rate, value.source.channels);
            this.origin?.free(); this.origin = candidate.fork(); this.edits = [];
            for (const next of value.edits) { applyStudioEdit(candidate, next.action, next); this.edits.push(edit(next.action, next)); }
            return candidate;
        } catch (error) { candidate.free(); this.clear(); throw error; }
    }
    clear() { this.origin?.free(); this.origin = null; this.edits = []; this.overflow = false; }
}
