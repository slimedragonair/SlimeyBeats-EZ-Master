import test from "node:test";
import assert from "node:assert/strict";
import { pcmRecordWav } from "./pcm-record-wav.js";
import { migrateLocalStudio } from "./studio-migration.js";

function record({ channels = 2, frames = 3, rate = 48000, data } = {}) {
    const bytes = new Uint8Array(frames * channels * 4), view = new DataView(bytes.buffer);
    for (let index = 0; index < frames * channels; index++) view.setFloat32(index * 4, (index - 2) / 8, true);
    data ??= new Blob([bytes]);
    for (const method of ["arrayBuffer", "stream", "text"])
        Object.defineProperty(data, method, { configurable: true, value() { assert.fail(`Whole origin Blob.${method} must not be called`); } });
    const value = { rate, channels, frames, data };
    Object.defineProperty(value, "getChannelData", { get() { assert.fail("Migration must not request channel planes"); } });
    return value;
}

async function inspect(file, audio) {
    assert.equal(file.type, "audio/wav");
    assert.equal(file.size, 44 + audio.data.size);
    // Permit only explicit bounded slices; neither migration nor its uploader
    // mock may flatten a complete audio Blob or File.
    file.arrayBuffer = () => assert.fail("Whole WAV read is forbidden");
    const header = await file.slice(0, 44).arrayBuffer(), view = new DataView(header);
    const id = offset => String.fromCharCode(...new Uint8Array(header, offset, 4));
    assert.deepEqual([id(0), id(8), id(12), id(36)], ["RIFF", "WAVE", "fmt ", "data"]);
    assert.equal(view.getUint32(4, true), file.size - 8);
    assert.equal(view.getUint32(16, true), 16);
    assert.equal(view.getUint16(20, true), 3);
    assert.equal(view.getUint16(22, true), audio.channels);
    assert.equal(view.getUint32(24, true), audio.rate);
    assert.equal(view.getUint32(28, true), audio.rate * audio.channels * 4);
    assert.equal(view.getUint16(32, true), audio.channels * 4);
    assert.equal(view.getUint16(34, true), 32);
    assert.equal(view.getUint32(40, true), audio.data.size);
}

function local(recipe) {
    return { calls: [], async call(action) {
        this.calls.push(action);
        assert.equal(action, "recovery-snapshot", "Never commit, discard, export, clear or stop the local workspace");
        return recipe;
    } };
}

function remote() {
    return { events: [], async import(file) { this.events.push(["import", file]); return { stage: "source" }; },
        async importFiles(entries) { this.events.push(["importFiles", entries]); return { stage: "stems" }; },
        async call(action, values) { this.events.push([action, values]); return { stage: action, info: { controls: values.settings } }; },
        async cancel() { this.events.push(["cancel"]); } };
}

test("record encoder composes a format-3 header and exact immutable mono/stereo PCM, including Blob-only hosts", async () => {
    const NativeFile = globalThis.File;
    try {
        for (const fallback of [false, true]) for (const channels of [1, 2]) {
            globalThis.File = fallback ? undefined : NativeFile;
            const audio = record({ channels });
            const file = pcmRecordWav(audio, "origin.wav");
            assert.equal(file.name, "origin.wav");
            await inspect(file, audio);
            const actual = new DataView(await file.slice(44, 44 + audio.data.size).arrayBuffer());
            for (let index = 0; index < audio.frames * channels; index++) assert.equal(actual.getFloat32(index * 4, true), (index - 2) / 8);
        }
    } finally { globalThis.File = NativeFile; }
});

test("PCM geometry, duration, bytes and Blob identity fail before any remote import", async () => {
    const audio = record();
    for (const change of [{ rate: 7999 }, { rate: 192001 }, { rate: 8000.5 }, { channels: 0 }, { channels: 3 },
        { frames: 0 }, { frames: 1.5 }, { frames: NaN }, { frames: Number.MAX_SAFE_INTEGER + 1 },
        { rate: 8000, frames: 8000 * 900 + 1 }, { rate: 192000, frames: 128 * 1048576 / 8 + 1 },
        { data: new Uint8Array(24) }, { data: new Blob([new Uint8Array(23)]) }]) {
        const client = remote();
        await assert.rejects(migrateLocalStudio(local({ source: { ...audio, ...change }, stems: [], edits: [] }), client));
        assert.deepEqual(client.events, []);
    }
    for (const rate of [8000, 192000]) assert.doesNotThrow(() => pcmRecordWav({ ...audio, rate }));
});

test("source origin uploads once and returns the final staged result without local mutation or automatic export", async () => {
    const audio = record(), snapshot = { source: audio, stems: [], edits: [] };
    const client = remote(), source = local(snapshot), result = { remote: true, info: { controls: { unchanged: true } } };
    client.import = async file => { await inspect(file, audio); client.events.push(["import", file.name]); return result; };
    assert.equal(await migrateLocalStudio(source, client), result);
    assert.deepEqual(source.calls, ["recovery-snapshot"]);
    assert.deepEqual(client.events, [["import", "studio-source.wav"]]);
    assert.equal(snapshot.source, audio);
    assert.deepEqual(snapshot.edits, []);
});

test("bank labels/kinds and every ordered action payload, including undo/redo, survive replay without WASM proposal IDs", async () => {
    const controls = { tracks: [{ kind: "part_4294967295", gain_db: -3, balance: .25, mute: false, solo: true }],
        output_gain_db: -1, protect_peaks: true, ceiling_dbtp: -1 };
    const edits = [
        { action: "mix", settings: controls, proposal: 1234 },
        { action: "voice", target: "part_4294967295", settings: { deess: .3, noise_region: { start_seconds: 0, end_seconds: .1 } } },
        { action: "repair", target: "preview", selection: { start_seconds: .1, end_seconds: .2, low_hz: 80, high_hz: 500, reduction_db: -6, mode: "attenuate" } },
        { action: "doctor", target: "source", repair: { action: "declip", strength: .2 } },
        { action: "replace", kind: "part_4294967295" },
        { action: "region", kind: "drums", edit: { start_seconds: 0, end_seconds: .2, operation: { kind: "gain", db: -3 } } },
        { action: "history", redo: false }, { action: "history", redo: true },
    ];
    const snapshot = { source: null, stems: [{ kind: "part_4294967295", name: "Harmony / 左", audio: record() },
        { kind: "drums", name: "Drums. Original", audio: record() }], edits };
    const saved = structuredClone(edits), source = local(snapshot), client = remote();
    const result = { remote: true, info: { controls, can_redo: false } };
    client.importFiles = async entries => {
        assert.deepEqual(entries.map(({ kind, name, file }) => ({ kind, name, filename: file.name })),
            [{ kind: "part_4294967295", name: "Harmony / 左", filename: "part_4294967295.wav" }, { kind: "drums", name: "Drums. Original", filename: "drums.wav" }]);
        for (let index = 0; index < entries.length; index++) await inspect(entries[index].file, snapshot.stems[index].audio);
        client.events.push(["bank"]); return { stage: "origin" };
    };
    client.call = async (action, values) => {
        client.events.push([action, structuredClone(values)]);
        if (action === "mix") values.settings.tracks[0].gain_db = 12;
        return result;
    };
    assert.equal(await migrateLocalStudio(source, client), result);
    assert.deepEqual(client.events, [["bank"], ...saved.map(({ action, proposal, ...values }) => [action, values])]);
    assert.deepEqual(edits, saved, "Remote mutation must not alter the local recipe or controls");
    assert.deepEqual(source.calls, ["recovery-snapshot"]);
});

test("overflow, malformed recipes, unknown kinds and invalid late edits reject before any upload", async () => {
    const audio = record(), stem = { kind: "part_12", name: "Twelve", audio };
    const invalid = [null, { stems: [], edits: [] }, { source: audio, stems: [], edits: [], overflow: true },
        { source: audio, stems: [], edits: Array.from({ length: 129 }, () => ({ action: "history", redo: false })) },
        { source: audio, stems: [], edits: [{ action: "voice", target: "source", settings: {} }, { action: "export" }] },
        { source: audio, stems: [stem], edits: [] },
        { source: null, stems: [stem, stem], edits: [] },
        { source: null, stems: [stem, { ...stem, kind: "drums", audio: record({ rate: 44100 }) }], edits: [] },
        ...["part_0", "part_01", "part_4294967296", "part_1\n", "vocal", "source", "unknown"].map(kind => ({ source: null, stems: [{ ...stem, kind }], edits: [] })),
        ...[{ action: "history", redo: 1 }, { action: "region", kind: "drums", edit: {} },
            { action: "voice", target: "part_13", settings: {} },
            { action: "mix", settings: { tracks: [{ kind: "drums" }] } }].map(edit => ({ source: null, stems: [stem], edits: [edit] }))];
    for (const recipe of invalid) {
        const client = remote();
        await assert.rejects(migrateLocalStudio(local(recipe), client));
        assert.deepEqual(client.events, []);
    }
    const client = remote(), failure = new Error("Recovery recipe overflow");
    await assert.rejects(migrateLocalStudio({ call: async () => { throw failure; } }, client), error => error === failure);
    assert.deepEqual(client.events, []);
});

test("aggregate origin bank budget applies even when all immutable Blobs share backing data", async () => {
    const data = new Blob([new Uint8Array(512 * 1024)]);
    const audio = record({ frames: data.size / 8, rate: 8000, data });
    const stems = Array.from({ length: 257 }, (_, index) => ({ kind: `part_${index + 1}`, name: `Part ${index + 1}`, audio }));
    const client = remote();
    await assert.rejects(migrateLocalStudio(local({ source: null, stems, edits: [] }), client), /128 MiB/);
    assert.deepEqual(client.events, []);
});

test("failed replay returns no publishable result and preserves the old local workspace", async () => {
    const recipe = { source: record(), stems: [], edits: [{ action: "voice", target: "source", settings: {} }, { action: "history", redo: false }] };
    const source = local(recipe), client = remote(), failure = new Error("Server edit failed");
    client.call = async () => { throw failure; };
    let published = false;
    await assert.rejects(migrateLocalStudio(source, client).then(() => { published = true; }), error => error === failure);
    assert.equal(published, false);
    assert.deepEqual(source.calls, ["recovery-snapshot"]);
    assert.equal(client.events.length, 1);
    assert.equal(recipe.edits.length, 2);
});

test("pre-abort and abort during snapshot never start or cancel server work", async () => {
    for (const before of [false, true]) {
        const controller = new AbortController(), client = remote();
        if (before) controller.abort();
        const source = { async call() { assert.equal(before, false); controller.abort(); return { source: record(), stems: [], edits: [] }; } };
        await assert.rejects(migrateLocalStudio(source, client, { signal: controller.signal }), { name: "AbortError" });
        assert.deepEqual(client.events, []);
    }
});

test("abort during upload or replay cancels the staged server once and stops all following steps", async () => {
    for (const during of ["import", "voice"]) {
        const controller = new AbortController(), client = remote();
        const recipe = { source: record(), stems: [], edits: [{ action: "voice", target: "source", settings: {} }, { action: "history", redo: false }] };
        client.import = async () => { client.events.push(["import"]); if (during === "import") controller.abort(); return {}; };
        client.call = async action => { client.events.push([action]); controller.abort(); return {}; };
        await assert.rejects(migrateLocalStudio(local(recipe), client, { signal: controller.signal }), { name: "AbortError" });
        assert.deepEqual(client.events, during === "import" ? [["import"], ["cancel"]] : [["import"], ["voice"], ["cancel"]]);
    }
});

test("finished migrations detach abort listeners and cancellation failures remain explicit", async () => {
    const recipe = { source: record(), stems: [], edits: [] }, controller = new AbortController(), client = remote();
    await migrateLocalStudio(local(recipe), client, { signal: controller.signal });
    controller.abort(); await Promise.resolve();
    assert.equal(client.events.length, 1);
    const failing = remote(), cancelled = new AbortController(), failure = new Error("Cancel request failed");
    failing.import = async () => { cancelled.abort(); return {}; };
    failing.cancel = async () => { throw failure; };
    await assert.rejects(migrateLocalStudio(local(recipe), failing, { signal: cancelled.signal }),
        error => error instanceof AggregateError && error.errors[1] === failure);
});

test("abort cancels an outstanding upload and waits for its server cancellation acknowledgement", async () => {
    const controller = new AbortController(), client = remote();
    let uploadStarted, releaseUpload, acknowledgeCancel;
    const started = new Promise(resolve => { uploadStarted = resolve; });
    client.import = () => { uploadStarted(); return new Promise(resolve => { releaseUpload = resolve; }); };
    client.cancel = () => { client.events.push(["cancel"]); releaseUpload({}); return new Promise(resolve => { acknowledgeCancel = resolve; }); };
    let settled = false;
    const migration = migrateLocalStudio(local({ source: record(), stems: [], edits: [] }), client, { signal: controller.signal });
    migration.then(() => { settled = true; }, () => { settled = true; });
    await started;
    controller.abort();
    await Promise.resolve(); await Promise.resolve();
    assert.deepEqual(client.events, [["cancel"]]);
    assert.equal(settled, false);
    acknowledgeCancel();
    await assert.rejects(migration, { name: "AbortError" });
});
