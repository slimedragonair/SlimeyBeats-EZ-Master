import { createArrangementClient } from "./arrangement-client.js";
import { createArrangementPreview } from "./arrangement-preview.js";
import { createArrangementImporter } from "./arrangement-import.js";
import { saveArrangementProject, openArrangementProject } from "./arrangement-project.js";
import { exportArrangementAudio, supportsArrangementMp3 } from "./arrangement-export.js";
import { createRemoteArrangementAdapter } from "./hybrid/arrangement-remote-adapter.js";

export function createArrangementAdapter({ mode = "local", remoteProject = null, onChange = () => {},
    beforePlay = () => {}, onError = () => {}, sample_rate = 48000, channels = 2 } = {}) {
    if (mode === "server") return createRemoteArrangementAdapter({ remoteProject, beforePlay, onChange, onError, sample_rate, channels });
    if (mode !== "local" || remoteProject) throw new Error("Choose a supported arrangement engine");
    const client = createArrangementClient();
    let transport = createArrangementPreview(client, { beforePlay, onError });
    let snapshot, defaults, closed = false;
    const ready = client.request("init", { sample_rate, channels, epoch: crypto.randomUUID() }).then(value => accept(value, true));
    const importer = createArrangementImporter(client, () => snapshot);
    function accept(value, notify = false) {
        if (closed) throw new Error("Arrangement adapter was cleared");
        defaults ||= value.defaults;
        value.defaults = defaults;
        value.capabilities.waveforms = value.capabilities.waveform;
        value.capabilities.import_zip = true;
        value.capabilities.import_formats = ["wav", "mp3"];
        value.capabilities.project = true;
        value.capabilities.mp3_export = supportsArrangementMp3(value);
        value.capabilities.export_formats = ["wav16", "wav24", "wav32f", ...(value.capabilities.mp3_export ? ["mp3"] : [])];
        value.capabilities.playback = !!(globalThis.AudioWorkletNode && (globalThis.AudioContext || globalThis.webkitAudioContext));
        if (!value.capabilities.playback) value.capabilities.playback_unavailable_reason = "This browser does not provide AudioWorklet playback.";
        const changed = !snapshot || snapshot.revision !== value.revision || snapshot.preview?.prepared_revision !== value.preview?.prepared_revision || snapshot.preview?.pending !== value.preview?.pending;
        snapshot = value; if (notify && changed) onChange(value); return value;
    }
    async function request(action, data, options) { await ready; return client.request(action, data, options); }
    function expected(options = {}) {
        const value = options.expectedRevision ?? snapshot.revision;
        if (value !== snapshot.revision) throw new Error("The arrangement changed. Refresh before editing this revision.");
        return value;
    }
    async function metadata(action, data, options) { return accept(await request(action, data, options), true); }
    return {
        async read(options = {}) { return accept(await request("read", {}, options)); },
        async reviewImport(files, options) { await ready; return importer.reviewImport(files, options); },
        discardImport(review, options) { return importer.discardImport(review, options); },
        async addImport(review, selection, options) { await ready; return accept(await importer.addImport(review, selection, options), true); },
        async edit(edits, options = {}) { await ready; return metadata("edit", { expectedRevision: expected(options), edits }, options); },
        async undo(options = {}) { await ready; return metadata("history", { expectedRevision: expected(options), redo: false }, options); },
        async redo(options = {}) { await ready; return metadata("history", { expectedRevision: expected(options), redo: true }, options); },
        waveform(asset_id, { start_frame, end_frame, bins = 512, signal } = {}) {
            return request("waveform", { asset_id, start_frame, end_frame, bins }, { signal });
        },
        eqResponse(effect, options = {}) { return request("eq-response", { effect }, options); },
        async suggestEq(target, options = {}) {
            await ready; return request("suggest-eq", { expectedRevision: expected(options), target,
                profile: options.profile || "neutral", exclude_eq_id: options.exclude_eq_id || "" }, options);
        },
        async previewSource(options = {}) {
            await ready; const revision = expected(options);
            const value = accept(await request("read", {}, options));
            if (value.revision !== revision) throw new Error("Preview revision was superseded");
            const source = await transport.adopt(value, options);
            const prepared = await request("read", {}, options);
            if (prepared.revision !== revision || prepared.preview.pending || prepared.preview.prepared_revision !== revision)
                throw new Error("Preview preparation was superseded; existing playback was kept");
            accept(prepared, true); return source;
        },
        async exportAudio(options = {}) {
            await ready; expected(options); return exportArrangementAudio(client, snapshot, options);
        },
        assetWav(asset_id, options) { return request("asset-wav", { asset_id }, options); },
        async saveProject(options = {}) {
            await ready; expected(options); return saveArrangementProject(client, snapshot, options);
        },
        async openProject(input, options = {}) {
            await ready;
            const value = await openArrangementProject(client, input, expected(options), options);
            transport.facade.clear();
            transport = createArrangementPreview(client, { beforePlay, onError });
            importer.clear(); return accept(value, true);
        },
        clear() { if (closed) return; closed = true; importer.clear(); transport.facade.clear(); client.close(); },
    };
}
