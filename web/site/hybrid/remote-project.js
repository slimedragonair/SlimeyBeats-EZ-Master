import { serverProcessing } from "./main-controls.js";
import { RemoteMediaBridge } from "./remote-media-bridge.js";

const ID = /^[A-Za-z0-9_-]{1,128}$/;
const pause = (ms, signal) => new Promise((resolve, reject) => {
    const timer = setTimeout(done, ms);
    function done() { signal?.removeEventListener("abort", abort); resolve(); }
    function abort() { clearTimeout(timer); reject(signal.reason); }
    signal?.addEventListener("abort", abort, { once: true });
    if (signal?.aborted) abort();
});
export const remoteAudio = asset => ({ ...asset, remote: true, audio_id: asset.id,
    length: asset.frames, sampleRate: asset.sample_rate, numberOfChannels: asset.channels,
    duration: asset.duration_seconds });

// Each workspace owns a remote document. Audio files and heavy DSP stay on the
// server; only metadata, waveform envelopes and streamed previews reach the UI.
export class RemoteProject {
    constructor({ progress = () => {}, changed = () => {} } = {}) {
        Object.assign(this, { progress, changed });
        this.manifest = null;
        this.pending = null;
        this.audioCache = new Map();
        this.epoch = 0;
    }
    get id() { return this.manifest?.id; }
    get revision() { return this.manifest?.revision || 0; }
    get loaded() { return !!this.manifest?.original_id; }
    async connect() {
        const client = await serverProcessing.connection();
        if (this.client !== client || this.connectionRevision !== client.connectionRevision) {
            this.media?.dispose();
            this.client = client;
            this.media = new RemoteMediaBridge(client);
            this.manifest = null;
            this.audioCache.clear();
        }
        this.connectionRevision = client.connectionRevision;
        client.onRemoteFailure = error => serverProcessing.failed(error);
        this.caps = await client.remoteRequest("/v2/capabilities");
        if (this.caps?.api_version !== 2 || this.caps.available === false || !Array.isArray(this.caps.operations))
            throw new Error("The server is updating. Full Studio processing is not ready yet; retry shortly.");
        await this.media.ready();
        return this.caps;
    }
    async ensure() {
        if (!this.client || !this.caps) await this.connect();
        if (!this.manifest) this.accept(await this.client.remoteRequest("/v2/projects", { method: "POST", body: {} }));
        return this.manifest;
    }
    path(suffix = "") {
        if (!ID.test(this.id || "")) throw new Error("Open a server project first.");
        return `/v2/projects/${this.id}${suffix}`;
    }
    accept(value) {
        if (value?.api_version !== 2 || !ID.test(value.id || "") ||
            !Number.isSafeInteger(value.revision) || value.revision < 0 || !Array.isArray(value.assets) ||
            value.assets.length > 2048 || value.assets.some(a => !ID.test(a.id || "") ||
                !Number.isSafeInteger(a.frames) || a.frames < 1 || ![1, 2].includes(a.channels) ||
                !Number.isFinite(a.duration_seconds) || a.duration_seconds <= 0 ||
                !Number.isInteger(a.sample_rate) || a.sample_rate < 8000 || a.sample_rate > 384000))
            throw new Error("The server returned an invalid project. Current audio was kept.");
        this.manifest = value;
        const retained = new Set(value.assets.map(asset => asset.id));
        for (const id of this.audioCache.keys()) if (!retained.has(id)) this.audioCache.delete(id);
        this.changed(value);
        return value;
    }
    async refresh() { return this.accept(await this.client.remoteRequest(this.path())); }
    async upload(file, role, signal, { label, kind } = {}) {
        signal?.throwIfAborted();
        if (!(file instanceof Blob) || !Number.isSafeInteger(file.size) || file.size <= 0)
            throw new Error("Choose a nonempty audio file or stem ZIP.");
        if (!this.caps) await this.connect(); signal?.throwIfAborted();
        const limit = this.caps.limits.max_upload_bytes;
        if (file.size > limit) throw new Error(`This file exceeds the server's ${(limit / 1048576).toFixed(0)} MiB upload limit.`);
        const extension = (file.name || "audio.wav").split(".").at(-1).toLowerCase();
        if (!/^(wav|mp3|flac|m4a|aac|aif|aiff|ogg|zip)$/.test(extension))
            throw new Error("Choose WAV, MP3, FLAC, M4A, AAC, AIFF, OGG or a stem ZIP.");
        await this.ensure(); signal?.throwIfAborted();
        // Preserve a useful local display label without sending source filenames.
        let upload = await this.client.remoteRequest(this.path("/uploads"), { method: "POST",
            body: { filename: `source.${extension}`, bytes: file.size, role,
                ...(role === "stem" && label ? { label: String(label).split(/[\\/]/).at(-1).replace(/[\x00-\x1f:]/g, "_").replaceAll("..", "_").slice(0, 128) } : {}),
                ...(role === "stem" && kind ? { kind } : {}) }, signal });
        if (!ID.test(upload?.id || "") || upload.total_bytes !== file.size || upload.uploaded_bytes !== 0)
            throw new Error("The server could not reserve this upload.");
        const chunk = Math.min(1048576, this.caps.limits.chunk_bytes);
        while (upload.uploaded_bytes < file.size) {
            signal?.throwIfAborted();
            const start = upload.uploaded_bytes, end = Math.min(start + chunk, file.size);
            upload = await this.client.remoteRequest(this.path(`/uploads/${upload.id}?offset=${start}`),
                { method: "PUT", body: file.slice(start, end), binary: true, signal });
            signal?.throwIfAborted();
            if (upload.uploaded_bytes !== end || upload.total_bytes !== file.size)
                throw new Error("The server did not acknowledge this upload chunk. Current work was kept.");
            this.progress(.35 * end / file.size, `Uploading to the server · ${Math.round(100 * end / file.size)}%`);
        }
        return upload.id;
    }
    async command(operation, values = {}, { signal, allowOff = false } = {}) {
        if (!serverProcessing.enabled && !allowOff)
            throw new Error("Turn on Server processing to edit this server project. Its previews and existing downloads remain available.");
        await this.ensure(); signal?.throwIfAborted();
        await this.refresh(); signal?.throwIfAborted();
        while (["queued", "running"].includes(this.manifest.command?.state)) {
            this.progress(this.manifest.command.progress || 0, `Server · ${this.manifest.command.message || "finishing the previous command"}`);
            await pause(700, signal); await this.refresh(); signal?.throwIfAborted();
        }
        const epoch = ++this.epoch;
        const current = value => {
            signal?.throwIfAborted();
            if (epoch !== this.epoch) throw new DOMException("Server command cancelled", "AbortError");
            return this.accept(value);
        };
        if (!this.caps.operations.includes(operation)) throw new Error("This tool is not available on the server yet.");
        const body = { operation, base_revision: this.revision, ...values };
        let result = current(await this.client.remoteRequest(this.path("/commands"), { method: "POST", body, signal }));
        const command = result.command?.id;
        if (!ID.test(command || "")) throw new Error("The server did not acknowledge this command.");
        while (["queued", "running"].includes(result.command?.state)) {
            signal?.throwIfAborted();
            this.progress(.35 + .6 * (result.command.progress || 0), `Server · ${result.command.message || operation}`);
            await pause(700, signal);
            result = current(await this.client.remoteRequest(this.path(), { signal }));
            if (result.command?.id !== command) throw new Error("The server project changed during processing. Refresh before editing.");
        }
        signal?.throwIfAborted();
        if (result.command?.state !== "completed")
            throw new Error(result.command?.state === "cancelled" ? "Server processing was cancelled; your saved audio was kept."
                : `The server could not complete ${operation.replaceAll("_", " ")}. Your saved audio was kept.`);
        this.progress(1, "Server result ready.");
        return result;
    }
    async import(file, role = "original", { signal, ...values } = {}) {
        await this.connect(); signal?.throwIfAborted();
        const upload = await this.upload(file, role, signal);
        return this.command(role === "reference" ? "reference" : role === "stem_bundle" ? "import_stems" : "import",
            { upload_ids: [upload], ...values }, { signal });
    }
    asset(id) {
        const asset = this.manifest?.assets.find(item => item.id === id);
        if (!asset) throw new Error("This server audio is unavailable or expired. Reopen the source to continue.");
        return remoteAudio(asset);
    }
    async audio(id) {
        if (!this.audioCache.has(id)) this.audioCache.set(id, this.#audio(id).catch(error => { this.audioCache.delete(id); throw error; }));
        return this.audioCache.get(id);
    }
    async #audio(id) {
        const asset = this.asset(id);
        const waveform = await this.client.remoteRequest(this.path(`/assets/${id}/waveform`));
        const peaks = waveform?.peaks;
        if (!Array.isArray(peaks) || peaks.length > 8192 || peaks.some(p => !Array.isArray(p) || p.length !== 2 || p.some(n => !Number.isFinite(n))))
            throw new Error("The server returned an invalid waveform.");
        asset.waveform = peaks;
        const fallback = 10 ** ((asset.analysis?.metrics?.sample_peak_dbfs ?? 0) / 20);
        asset.channelPeaks = Array.isArray(waveform.channel_peaks) && waveform.channel_peaks.length === asset.channels &&
            waveform.channel_peaks.every(n => Number.isFinite(n) && n >= 0) ? waveform.channel_peaks : new Array(asset.channels).fill(fallback);
        asset.mediaUrl = this.media.url(this.id, id);
        asset.project = this;
        return asset;
    }
    async export(operation, values, name, options) {
        const result = await this.command(operation, values, options);
        const artifact = result.artifacts?.find(a => a.id === result.command?.artifact_id) || result.artifacts?.at(-1);
        if (!artifact) throw new Error("The server did not return a download.");
        options?.signal?.throwIfAborted();
        await this.media.download(this.id, artifact, name);
        return artifact;
    }
    async cancel() {
        if (!this.id) return;
        ++this.epoch;
        this.accept(await this.client.remoteRequest(this.path("/cancel"), { method: "POST", body: {} }));
    }
    async close() {
        if (!this.id) return;
        const result = await this.client.remoteRequest(this.path(), { method: "DELETE" });
        if (result.deleted !== true) throw new Error("Server deletion was not confirmed. Retry cleanup.");
        this.manifest = null; this.audioCache.clear(); this.media.clear(); this.changed(null);
    }
}
