import { StudioClient } from "../studio-client.js";

// Translate the established Studio actions into server commands. UI controls
// and the native DSP types retain their existing names, units and semantics.
export class RemoteStudioClient {
    constructor(project) {
        this.project = project; this.defaultsClient = new StudioClient();
        this.beforeId = null; this.nameOverrides = new Map();
    }
    get loaded() { return !!this.project.manifest?.preview_id; }
    target(value = "source") {
        const manifest = this.project.manifest;
        if (!manifest) throw new Error("Open server Studio audio first.");
        const id = value === "source" ? manifest.original_id : value === "preview" ? manifest.preview_id
            : manifest.stems.find(stem => stem.kind === value)?.id;
        if (!id) throw new Error("The selected server audio is unavailable.");
        return id;
    }
    async result(manifest = this.project.manifest) {
        const before = manifest.before_id || this.beforeId || manifest.original_id || manifest.preview_id;
        const after = manifest.preview_id || manifest.original_id;
        const beforeAudio = await this.project.audio(before), previewAudio = await this.project.audio(after);
        const stemAudio = new Map();
        // Bound concurrent metadata requests independently of the bank size.
        for (const stem of manifest.stems) {
            const buffer = await this.project.audio(stem.id);
            stemAudio.set(stem.kind, { buffer, identity: stem.id, peaks: buffer.channelPeaks });
        }
        const names = Object.fromEntries(manifest.stems.map(stem => [stem.kind,
            this.nameOverrides.get(stem.kind) || stem.name || stem.kind]));
        return { remote: true, proposal: { revision: manifest.revision }, beforeAudio, previewAudio, stemAudio,
            info: { loaded: true, remote: true, stems: manifest.stems.map(s => s.kind), stem_names: names,
                channels: previewAudio.channels, sample_rate: previewAudio.sample_rate,
                duration_seconds: previewAudio.duration_seconds, frames: previewAudio.frames,
                controls: structuredClone(manifest.controls), preview_is_mix: manifest.preview_is_mix ?? this.previewIsMix ?? manifest.stems.length > 0,
                direct_edit_stems: manifest.edit_origin_kind ? [manifest.edit_origin_kind] : [],
                can_undo: manifest.can_undo, can_redo: manifest.can_redo,
                notes: ["Full-resolution audio and heavy tools run on the server. Playback streams a compressed preview; downloads are rendered from the full-resolution audio.",
                    "Server projects are temporary. Download a session ZIP to keep your work before leaving."] } };
    }
    async import(file) {
        this.controller = new AbortController();
        const manifest = await this.project.import(file, "original", { signal: this.controller.signal });
        this.beforeId = manifest.original_id; this.previewIsMix = false;
        return this.result(manifest);
    }
    async clone(audio) {
        await this.project.connect(); await this.project.ensure();
        const manifest = await this.project.command("clone", { source_project_id: audio.project.id, audio_id: audio.id });
        this.beforeId = manifest.original_id; this.previewIsMix = false;
        return this.result(manifest);
    }
    async importFiles(entries, preference = "wav") {
        this.controller = new AbortController(); const signal = this.controller.signal;
        await this.project.connect();
        const zip = entries.length === 1 && entries[0].file.name.toLowerCase().endsWith(".zip");
        const uploadIds = [];
        for (const entry of entries) uploadIds.push(await this.project.upload(entry.file, zip ? "stem_bundle" : "stem", signal,
            { label: entry.name || entry.file.name, kind: entry.kind }));
        const manifest = await this.project.command("import_stems", { upload_ids: uploadIds }, { signal });
        if (!zip) for (const entry of entries) if (entry.kind)
            this.nameOverrides.set(entry.kind, entry.name || entry.file.name.replace(/\.[^.]+$/, ""));
        this.beforeId = manifest.original_id || manifest.preview_id; this.previewIsMix = true;
        return this.result(manifest);
    }
    async call(action, values = {}, transfer = []) {
        if (action === "init") return this.defaultsClient.call("init");
        if (["import-plan", "archive-open", "archive-read", "archive-close"].includes(action))
            return this.defaultsClient.call(action, values, transfer);
        if (action === "load-file") return this.import(values.file);
        if (action === "clone-audio") return this.clone(values.audio);
        if (action === "import-files") return this.importFiles(values.entries, values.preference);
        if (action === "commit") return true;
        if (action === "discard") {
            if (values.proposal?.revision === this.project.revision) await this.project.command("history", { redo: false });
            return true;
        }
        if (action === "analyze" || action === "doctor-scan" || action === "spectrogram") {
            const operation = { analyze: "analyze", "doctor-scan": "issues", spectrogram: "spectrogram" }[action];
            const manifest = await this.project.command(operation, { audio_id: this.target(values.target) });
            return action === "analyze" ? manifest.analysis.metrics : action === "doctor-scan" ? manifest.issues : manifest.spectrogram;
        }
        if (action === "recovery-snapshot")
            throw new Error("Streamed server projects need a session ZIP backup. Local recovery never saves server access tokens.");
        let operation, params, target = values.target ? this.target(values.target) : null;
        switch (action) {
            case "mix": operation = "mix"; params = { mix: values.settings }; this.beforeId = this.project.manifest.original_id; this.previewIsMix = true; break;
            case "voice": operation = "voice_cleanup"; params = { audio_id: target, voice: values.settings }; break;
            case "repair": operation = "spectral_repair"; params = { audio_id: target, spectral: values.selection }; break;
            case "doctor": operation = "repair"; params = { audio_id: target, repair: values.repair }; break;
            case "replace": operation = "replace"; params = { audio_id: this.target(values.kind) }; this.previewIsMix = true; break;
            case "region": operation = "region_edit"; params = { audio_id: this.target(values.kind), region: values.edit }; this.previewIsMix = true; break;
            case "history": operation = "history"; params = { redo: values.redo }; break;
            case "separate": operation = "separate"; params = { audio_id: this.target("source"), separation_mode: values.mode }; this.previewIsMix = true; break;
            default: throw new Error("This Studio action is not available on the server.");
        }
        if (target) { this.beforeId = target; this.previewIsMix = false; }
        this.controller = new AbortController();
        this.active = { operation, baseRevision: this.project.revision };
        try {
            const manifest = await this.project.command(operation, params, { signal: this.controller.signal });
            return await this.result(manifest);
        } finally { this.active = null; }
    }
    async cancel() {
        const active = this.active;
        await this.project.cancel();
        const command = this.project.manifest?.command;
        if (active && command?.state === "completed" && command.operation === active.operation &&
            command.base_revision === active.baseRevision && this.project.manifest.can_undo)
            await this.project.command("history", { redo: false });
        this.controller?.abort(new DOMException("Studio processing cancelled", "AbortError"));
    }
    stop() { this.defaultsClient.stop(); this.controller?.abort(new DOMException("Studio stopped", "AbortError")); }
    async close() { this.stop(); await this.project.close(); }
}
