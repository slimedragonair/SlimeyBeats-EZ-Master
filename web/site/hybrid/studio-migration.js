import { STEM_KINDS } from "../studio-stems.js";
import { LOCAL_STUDIO_PCM_BYTES, pcmRecordWav } from "./pcm-record-wav.js";
const MAX_EDITS = 128;
const MAX_STEMS = 1024;

function stemKind(kind) {
    if (STEM_KINDS.includes(kind)) return true;
    if (typeof kind !== "string" || kind.length > 15 || !kind.startsWith("part_")) return false;
    const number = Number(kind.slice(5));
    return Number.isInteger(number) && number >= 1 && number <= 0xffffffff && kind === `part_${number}`;
}

function object(value) { return value && typeof value === "object" && !Array.isArray(value); }

function replayEdit(edit, kinds) {
    if (!object(edit)) throw new Error("Invalid Studio migration edit.");
    const { action } = edit;
    const target = value => value === "source" || value === "preview" || kinds.has(value);
    const requiredObject = value => {
        if (!object(value)) throw new Error(`Invalid Studio ${action} payload.`);
        return value;
    };
    let values;
    switch (action) {
        case "mix": {
            const settings = requiredObject(edit.settings);
            if (settings.tracks !== undefined && (!Array.isArray(settings.tracks) ||
                settings.tracks.some(track => !object(track) || !kinds.has(track.kind)) ||
                new Set(settings.tracks.map(track => track.kind)).size !== settings.tracks.length))
                throw new Error("Studio mix refers to unknown or duplicate stem kinds.");
            values = { settings };
            break;
        }
        case "voice": case "repair": case "doctor": {
            if (!target(edit.target)) throw new Error("Studio edit refers to an unknown target.");
            const field = { voice: "settings", repair: "selection", doctor: "repair" }[action];
            values = { target: edit.target, [field]: requiredObject(edit[field]) };
            break;
        }
        case "replace": case "region":
            if (!kinds.has(edit.kind)) throw new Error("Studio edit refers to an unknown stem kind.");
            values = { kind: edit.kind, ...(action === "region" ? { edit: requiredObject(edit.edit) } : {}) };
            break;
        case "history":
            if (typeof edit.redo !== "boolean") throw new Error("Invalid Studio history direction.");
            values = { redo: edit.redo };
            break;
        default: throw new Error("Unsupported Studio migration edit.");
    }
    // Remote calls may retain/mutate their arguments. Leave the local recipe,
    // including its settings and stale WASM proposal IDs, entirely untouched.
    return { action, values: structuredClone(values) };
}

function prepare(recipe) {
    if (!object(recipe) || recipe.overflow || !Array.isArray(recipe.stems) || recipe.stems.length > MAX_STEMS ||
        !Array.isArray(recipe.edits) || recipe.edits.length > MAX_EDITS)
        throw new Error("Studio migration requires a complete recovery recipe within 128 edits.");
    if (recipe.stems.length && recipe.source != null)
        throw new Error("Studio migration requires one unambiguous origin.");
    const kinds = new Set();
    let total = 0;
    const entries = recipe.stems.map(stem => {
        if (!object(stem) || !stemKind(stem.kind) || kinds.has(stem.kind) ||
            (stem.name !== undefined && typeof stem.name !== "string"))
            throw new Error("Invalid or duplicate Studio migration stem kind/name.");
        kinds.add(stem.kind);
        const file = pcmRecordWav(stem.audio, `${stem.kind}.wav`);
        const first = recipe.stems[0].audio;
        if (stem.audio.rate !== first.rate || stem.audio.frames !== first.frames || stem.audio.channels !== first.channels)
            throw new Error("Studio migration stems must share frames, rate and channels.");
        total += stem.audio.data.size;
        if (total > LOCAL_STUDIO_PCM_BYTES) throw new Error("Studio migration bank exceeds the 128 MiB origin limit.");
        return { file, kind: stem.kind, name: stem.name ?? stem.kind };
    });
    return { entries, source: entries.length ? null : pcmRecordWav(recipe.source),
        edits: recipe.edits.map(edit => replayEdit(edit, kinds)) };
}

/** Stage into a fresh remote client; the caller publishes only the returned result.
 * Pending UI controls remain the caller's state. Only committed recipe edits replay.
 * The caller keeps this client's intermediate project metadata out of the UI.
 */
export async function migrateLocalStudio(localClient, remoteClient, { signal } = {}) {
    let remoteStarted = false, cancellation, cancellationError;
    const check = () => signal?.throwIfAborted();
    const abort = () => {
        if (remoteStarted && !cancellation) cancellation = Promise.resolve()
            .then(() => remoteClient.cancel()).catch(error => { cancellationError = error; });
    };
    check();
    signal?.addEventListener("abort", abort, { once: true });
    try {
        const snapshot = await localClient.call("recovery-snapshot");
        check();
        const recipe = prepare(snapshot);
        check();
        remoteStarted = true;
        let result = recipe.entries.length ? await remoteClient.importFiles(recipe.entries) : await remoteClient.import(recipe.source);
        check();
        for (const { action, values } of recipe.edits) {
            check();
            result = await remoteClient.call(action, values);
            check();
        }
        return result;
    } catch (error) {
        if (signal?.aborted) {
            abort();
            await cancellation;
            if (cancellationError) throw new AggregateError([signal.reason, cancellationError],
                "Studio migration was cancelled; server cancellation could not be confirmed.");
            check();
        }
        throw error;
    } finally { signal?.removeEventListener("abort", abort); }
}
