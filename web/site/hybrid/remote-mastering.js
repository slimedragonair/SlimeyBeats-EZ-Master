import { RemoteProject } from "./remote-project.js";
import { remoteSettings } from "./remote-settings.js";

export class RemoteMastering {
    constructor(progress) { this.project = new RemoteProject({ progress }); this.generation = 0; }
    begin() { this.controller = new AbortController(); return this.controller.signal; }
    async import(file) {
        const signal = this.begin();
        const manifest = file.remote
            ? (await this.project.connect(), await this.project.ensure(), await this.project.command("clone", { source_project_id: file.project.id, audio_id: file.id }, { signal }))
            : await this.project.import(file, "original", { signal });
        signal.throwIfAborted();
        const original = await this.project.audio(manifest.original_id);
        signal.throwIfAborted(); this.original = original;
        this.master = null;
        this.sourceFile = file;
        return this.original;
    }
    async analyze(settings) {
        const result = await this.project.command("analyze", { audio_id: this.original.id, settings: remoteSettings(settings) }, { signal: this.begin() });
        return result.analysis;
    }
    async reference(file) {
        const result = await this.project.import(file, "reference", { signal: this.begin() });
        this.referenceFile = file;
        this.referenceId = result.reference_id;
        return result;
    }
    async masterTrack(settings) {
        const signal = this.begin();
        const result = await this.project.command("master", { audio_id: this.original.id,
            reference_id: settings.reference ? this.referenceId : null, settings: remoteSettings(settings) }, { signal });
        signal.throwIfAborted();
        const buffer = await this.project.audio(result.master_id || result.preview_id);
        signal.throwIfAborted();
        this.master = buffer;
        return { buffer, report: result.report, raw_analysis: result.report.output };
    }
    async suggest(settings) {
        const result = await this.project.command("suggest", { audio_id: (this.master || this.original).id,
            settings: remoteSettings(settings) }, { signal: this.begin() });
        const proposal = result.suggestion || result.command?.result?.suggestion || result.command?.result;
        if (!Array.isArray(proposal?.bands) || proposal.bands.length !== 7)
            throw new Error("The server returned an invalid EQ suggestion. Current EQ was kept.");
        return { bands: proposal.bands, notes: Array.isArray(proposal.notes) ? proposal.notes : [],
            high_frequency: proposal.high_frequency ?? null };
    }
    async export(settings, format, bundle, name) {
        const signal = this.begin();
        await this.project.export(bundle ? "bundle" : "export", { audio_id: this.master.id,
            settings: remoteSettings(settings), format, ...(bundle ? { kind: "mastering", include_stems: false } : {}) },
            name, { signal });
        return this.project.manifest.report;
    }
    async cancel() { this.controller?.abort(new DOMException("Server processing cancelled", "AbortError")); await this.project.cancel(); }
    async close() { this.controller?.abort(new DOMException("Server project closed", "AbortError")); await this.project.close(); this.original = this.master = null; }
}
