import { RemoteProject } from "./remote-project.js";
import { remoteMediaPath } from "./remote-media-policy.js";
import { createArrangementStreamPreview } from "./arrangement-stream-preview.js";
import { createRemoteArrangementImport } from "./arrangement-remote-import.js";

const ID = /^[A-Za-z0-9_-]{1,128}$/;
export function checkedRemoteArrangement(value) {
    if (!value || !ID.test(value.epoch || "") || !Number.isSafeInteger(value.revision) || value.revision < 0 ||
        !Array.isArray(value.arrangement?.lanes) || !Array.isArray(value.arrangement.eq) ||
        !Number.isSafeInteger(value.arrangement.sample_rate) || value.arrangement.sample_rate < 8000 || value.arrangement.sample_rate > 384000 ||
        ![1, 2].includes(value.arrangement.channels) || !Array.isArray(value.assets) || !value.capabilities ||
        !Array.isArray(value.defaults?.bands) || value.defaults.bands.length !== 7 ||
        !Number.isSafeInteger(value.preview?.frames) || value.preview.frames < 0 || typeof value.preview.identity !== "string" ||
        value.assets.some(asset => !ID.test(asset.id || "") || !Number.isSafeInteger(asset.frames) || asset.frames <= 0 ||
            ![1, 2].includes(asset.channels) || !Number.isSafeInteger(asset.sample_rate)))
        throw new Error("The server returned incomplete arrangement metadata. Current work was kept.");
    if (new Set(value.assets.map(asset => asset.id)).size !== value.assets.length) throw new Error("The server returned duplicate source identities.");
    return { ...value, mode: "server" };
}

// This project contains only this arrangement's owned assets. The legacy stem
// workspace has its own RemoteProject and is never used as an aligned bank.
export function createRemoteArrangementAdapter({ remoteProject = null, beforePlay = () => {},
    onChange = () => {}, onError = () => {}, sample_rate = 48000, channels = 2 } = {}) {
    const project = remoteProject || new RemoteProject();
    let snapshot, ready, closed = false, connectionRevision, queue = Promise.resolve();
    function serial(action) {
        const result = queue.then(action);
        queue = result.catch(() => {});
        return result;
    }
    const path = () => project.path("/arrangement");
    const client = () => {
        if (closed) throw new Error("The server arrangement adapter is closed.");
        if (project.client.connectionRevision !== connectionRevision) throw new Error("The server connection changed. This arrangement was kept; reconnect its workspace explicitly.");
        return project.client;
    };
    function accept(value, notify = false) {
        const next = checkedRemoteArrangement(value);
        if (snapshot && next.epoch === snapshot.epoch && next.revision < snapshot.revision) throw new Error("A newer server arrangement revision is already displayed.");
        const changed = !snapshot || next.epoch !== snapshot.epoch || next.revision !== snapshot.revision ||
            next.preview.prepared_revision !== snapshot.preview.prepared_revision || next.preview.pending !== snapshot.preview.pending;
        snapshot = next; if (notify && changed) onChange(next); return next;
    }
    function initialise() {
        if (!ready) ready = (async () => {
            await project.connect(); const existing = !!project.id;
            await project.ensure(); connectionRevision = project.client.connectionRevision;
            let value;
            if (!existing) {
                value = await client().arrangementRequest(path(), { method: "POST", body: { operation: "create", sample_rate, channels } });
            } else {
                try { value = await client().arrangementRequest(path()); }
                catch (error) {
                    if (error.status !== 409) throw error;
                    value = await client().arrangementRequest(path(), { method: "POST", body: { operation: "create", sample_rate, channels } });
                }
            }
            return accept(value);
        })();
        return ready;
    }
    async function read(options = {}) { await initialise(); return serial(async () => accept(await client().arrangementRequest(path(), options))); }
    async function request(operation, data = {}, options = {}) {
        await initialise(); options.signal?.throwIfAborted();
        const expected = options.expectedRevision ?? snapshot.revision, epoch = options.epoch ?? snapshot.epoch;
        if (operation !== "stop" && (expected !== snapshot.revision || epoch !== snapshot.epoch)) throw new Error("The server arrangement changed. Refresh before applying this edit.");
        return serial(async () => {
            let revision = expected;
            if (operation !== "stop" && (snapshot.epoch !== epoch || snapshot.revision !== expected)) {
                if (operation === "waveform" && snapshot.epoch === epoch && snapshot.assets.some(asset => asset.id === data.asset_id)) revision = snapshot.revision;
                else throw new DOMException("The arrangement revision changed while this request was waiting.", "AbortError");
            }
            const value = await client().arrangementRequest(path(), { method: "POST", signal: options.signal,
                compact: operation === "prepare" || operation === "stop" || operation === "eq_response",
                body: { operation, epoch, expected_revision: revision, ...data } });
            return ["edit", "history", "add_uploads"].includes(operation) ? accept(value, true) : value;
        });
    }
    const stream = createArrangementStreamPreview({ beforePlay, onError,
        prepare: async (value, options) => {
            const descriptor = await request("prepare", value, options);
            if (descriptor.side_duration_frames && descriptor.stream_path !== remoteMediaPath(project.id, descriptor.stream_id, "arrangement"))
                throw new Error("The server returned an unowned arrangement stream path.");
            return descriptor;
        },
        stop: descriptor => request("stop", { stream_id: descriptor.stream_id }, { expectedRevision: snapshot.revision, epoch: descriptor.epoch }),
        mediaUrl: descriptor => project.media.url(project.id, descriptor.stream_id, "arrangement"),
    });
    const importer = createRemoteArrangementImport({ project, getSnapshot: () => snapshot,
        commit: (value, options) => request("add_uploads", value, options) });
    return {
        read,
        async reviewImport(files, options) { await initialise(); return importer.reviewImport(files, options); },
        discardImport: (review, options) => importer.discardImport(review, options),
        async addImport(review, selection, options) { await initialise(); return importer.addImport(review, selection, options); },
        edit: (edits, options) => request("edit", { edits: Array.isArray(edits) ? edits : [edits] }, options),
        undo: options => request("history", { redo: false }, options),
        redo: options => request("history", { redo: true }, options),
        waveform: (asset_id, { start_frame, end_frame, bins = 512, ...options }) =>
            request("waveform", { asset_id, range: { start_frame, end_frame }, bins }, options),
        eqResponse: (effect, options) => request("eq_response", { effect }, options),
        async previewSource(options = {}) {
            const value = await read({ signal: options.signal });
            if (options.expectedRevision !== undefined && value.revision !== options.expectedRevision) throw new Error("Server preview revision was superseded.");
            const source = await stream.adopt(value, options);
            const current = await read({ signal: options.signal });
            if (current.revision !== value.revision || current.epoch !== value.epoch || current.preview.pending || current.preview.prepared_revision !== value.revision)
                throw new Error("Server preview preparation was superseded. Current work was kept.");
            return source;
        },
        clear() { if (closed) return; stream.facade.clear(); importer.clear(); project.media?.dispose(); closed = true; },
    };
}
