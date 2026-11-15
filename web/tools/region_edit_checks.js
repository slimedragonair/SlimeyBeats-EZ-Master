import { assert, equal, fixture, region, checkEdit, wave, base64, rejects, operations, invalidEdits } from "./region_edit_pcm_checks.js";

function directState(studio) {
    return { info: studio.info(), preview: studio.current_wav32(),
        bank: JSON.parse(studio.info()).stems.map(kind => ({ kind, bytes: studio.stem_wav(kind) })) };
}
function sameState(actual, expected, label, metadata = true) {
    if (metadata) assert(actual.info === expected.info, `${label}: metadata/history changed`);
    equal(actual.preview, expected.preview, `${label}: preview`);
    assert(actual.bank.length === expected.bank.length, `${label}: bank length`);
    actual.bank.forEach((stem, i) => {
        assert(stem.kind === expected.bank[i].kind, `${label}: stem order`);
        equal(stem.bytes, expected.bank[i].bytes, `${label}: ${stem.kind}`);
    });
}
function stage(studio, channels = 2, frames = 8001) {
    for (const [kind, seed] of [["vocals", 0], ["part_9", 1]]) {
        const audio = fixture(channels, frames, 8000, seed);
        studio.stage_stem(kind, audio.samples, audio.rate, audio.channels);
    }
    studio.commit_stems();
    studio.set_stem_display_name("part_9", "Independent right/left fixture");
}
async function directChecks(engine, checks) {
    assert(typeof engine.BrowserStudio.prototype.edit_region === "function", "Fresh WASM candidate required: BrowserStudio.edit_region is absent");
    for (const channels of [1, 2]) {
        const studio = new engine.BrowserStudio();
        try {
            stage(studio, channels);
            const original = directState(studio);
            for (const target of ["source", "preview", "drums", "unknown", ""])
                await rejects(() => studio.edit_region(target, JSON.stringify(region("silence"))), `Invalid bank target ${target}`);
            await rejects(() => studio.edit_region("vocals", "{broken json"), "Malformed region JSON");
            sameState(directState(studio), original, "Rejected targets/JSON");
            for (const kind of operations) {
                const fork = studio.fork();
                try {
                    const edit = region(kind);
                    fork.edit_region("vocals", JSON.stringify(edit));
                    const after = directState(fork), source = wave(original.bank.find(s => s.kind === "vocals").bytes);
                    checkEdit(wave(after.bank.find(s => s.kind === "vocals").bytes).samples, source.samples, edit, channels, source.rate, `WASM ${channels}ch ${kind}`);
                    equal(after.bank.find(s => s.kind === "part_9").bytes, original.bank.find(s => s.kind === "part_9").bytes, `${kind}: other raw stem`);
                    sameState(directState(studio), original, `${kind}: parent fork isolation`);
                    fork.history(false); sameState(directState(fork), original, `${kind}: undo`, false);
                    fork.history(true); sameState(directState(fork), after, `${kind}: redo`);
                    for (const bad of invalidEdits()) {
                        await rejects(() => fork.edit_region("vocals", JSON.stringify(bad)), `${kind}: invalid region`);
                        sameState(directState(fork), after, `${kind}: rejected request`);
                    }
                } finally { fork.free(); }
            }
            checks.push(`WASM ${channels}ch: all five operations; exact outside region/other stem; channel order; fork isolation; exact undo/redo; invalid state retention`);
            for (const [start, end] of [[0, 1 / 8000], [8000 / 8000, 8001 / 8000], [1600.5 / 8000, 4000.5 / 8000]]) {
                for (const kind of operations) {
                    const fork = studio.fork(), edit = region(kind, start, end);
                    try {
                        fork.edit_region("vocals", JSON.stringify(edit));
                        checkEdit(wave(fork.stem_wav("vocals")).samples, fixture(channels).samples, edit, channels, 8000, `boundary ${kind}`);
                    } finally { fork.free(); }
                }
            }
            checks.push(`WASM ${channels}ch: first/last single frame, one-frame fades, half-frame rounding and exclusive end`);
            for (const db of [-60, 0, 12]) {
                const fork = studio.fork(), edit = region("gain");
                edit.operation.db = db;
                try {
                    fork.edit_region("vocals", JSON.stringify(edit));
                    checkEdit(wave(fork.stem_wav("vocals")).samples, fixture(channels).samples, edit, channels, 8000, `gain bound ${db}`);
                } finally { fork.free(); }
            }
        } finally { studio.free(); }
    }
}
async function load(client, frames = 8001, channels = 2) {
    const stems = [["vocals", 0], ["part_9", 1]].map(([kind, seed]) => ({ kind, display_name: kind,
        ...fixture(channels, frames, 8000, seed) }));
    const result = await client.call("stems", { stems });
    await client.call("commit", { proposal: result.proposal });
    return result;
}
async function workerState(client) {
    const [state, working, bank] = await Promise.all([client.call("init"), client.call("working"), client.call("stem-export")]);
    return { info: JSON.stringify(state.info), preview: working.bytes,
        bank: bank.files.map(file => ({ kind: file.name.slice(0, -4), bytes: file.bytes })) };
}
async function mutate(client, action, values) {
    const next = await client.call(action, values);
    assert(Number.isInteger(next.proposal), `${action}: missing proposal`);
    await client.call("commit", { proposal: next.proposal });
    return next;
}
function timedClient(StudioClient) {
    const client = new StudioClient(), call = client.call.bind(client);
    client.call = async (action, ...args) => {
        let timer;
        try {
            return await Promise.race([call(action, ...args), new Promise((_, reject) => {
                timer = setTimeout(() => {
                    const error = new Error(`Studio worker timed out after 30 seconds: ${action}`);
                    client.stop(error); reject(error);
                }, 30000);
            })]);
        } finally { clearTimeout(timer); }
    };
    return client;
}
async function workerChecks(StudioClient, clientUrl, checks) {
    const client = timedClient(StudioClient);
    try {
        await client.call("init");
        for (const channels of [1, 2]) {
            for (const kind of operations) {
                await load(client, 8001, channels);
                const before = await workerState(client), edit = region(kind);
                for (const target of ["source", "preview", "drums", "unknown"])
                    await rejects(() => client.call("region", { kind: target, edit }), `Invalid worker stem ${target}`);
                sameState(await workerState(client), before, "Rejected worker targets");
                const proposal = await client.call("region", { kind: "vocals", edit });
                sameState(await workerState(client), before, `${kind}: proposal not published`);
                equal(proposal.before, wave(before.preview).samples, `${kind}: original comparison PCM`);
                assert(proposal.stems.find(s => s.kind === "vocals")?.samples instanceof Float32Array, `${kind}: changed stem PCM missing`);
                const untouched = proposal.stems.find(s => s.kind === "part_9");
                assert(untouched && !untouched.samples, `${kind}: untouched stem unnecessarily transferred`);
                checkEdit(proposal.stems.find(s => s.kind === "vocals").samples, fixture(channels).samples, edit, channels, 8000, `worker ${kind}`);
                await rejects(() => client.call("commit", { proposal: proposal.proposal + 1000 }), "stale commit");
                await rejects(() => client.call("region", { kind: "vocals", edit }), "unsettled proposal overlap");
                await client.call("discard", { proposal: proposal.proposal });
                sameState(await workerState(client), before, `${kind}: discarded proposal`);
                assert((await client.call("recovery-snapshot")).edits.length === 0, "Discarded edit entered recovery recipe");
                await rejects(() => client.call("commit", { proposal: proposal.proposal }), "commit after discard");
                await mutate(client, "region", { kind: "vocals", edit });
                const after = await workerState(client);
                checkEdit(wave(after.bank.find(s => s.kind === "vocals").bytes).samples, fixture(channels).samples, edit, channels, 8000, `committed ${kind}`);
                equal(after.bank.find(s => s.kind === "part_9").bytes, before.bank.find(s => s.kind === "part_9").bytes, "Other stem changed at commit");
                await mutate(client, "history", { redo: false });
                sameState(await workerState(client), before, `${kind}: worker undo`, false);
                for (const bad of invalidEdits()) {
                    const retained = await workerState(client);
                    await rejects(() => client.call("region", { kind: "vocals", edit: bad }), "invalid worker region");
                    sameState(await workerState(client), retained, "invalid worker region retains redo");
                }
                await mutate(client, "history", { redo: true });
                sameState(await workerState(client), after, `${kind}: worker redo`);
            }
            checks.push(`worker ${channels}ch: five operations; real proposals, stale/overlapping rejection, discard/commit, changed-stem transfer, exact raw bank and undo/redo after invalid requests`);
        }
        await load(client);
        for (const kind of operations) await mutate(client, "region", { kind: "vocals", edit: region(kind) });
        await mutate(client, "history", { redo: false });
        const expected = await workerState(client), recipe = await client.call("recovery-snapshot");
        assert(recipe.edits.filter(e => e.action === "region").length === 5 && recipe.edits.at(-1).action === "history", "Recovery omitted region/history actions");
        const restored = timedClient(StudioClient);
        try {
            await restored.call("init");
            const proposal = await restored.call("recover", { recipe: structuredClone(recipe) });
            await restored.call("commit", { proposal: proposal.proposal });
            sameState(await workerState(restored), expected, "Fresh worker exact recovery");
            await mutate(restored, "history", { redo: true });
            await mutate(client, "history", { redo: true });
            sameState(await workerState(restored), await workerState(client), "Recovered redo branch");
            for (let i = 0; i < 2; i++) {
                await mutate(restored, "history", { redo: false });
                await mutate(client, "history", { redo: false });
                sameState(await workerState(restored), await workerState(client), `Recovered retained undo ${i}`);
            }
            const retained = await workerState(restored);
            await rejects(() => restored.call("history", { redo: false }), "Browser history cap must remain two");
            sameState(await workerState(restored), retained, "Exhausted undo retains state");
        } finally { restored.stop(); }
        checks.push("fresh worker recovery exactly replays all five region operations plus history, preserves redo and the existing two-preview undo cap");

        await load(client, 513);
        for (let i = 0; i < 128; i++) await mutate(client, "region", { kind: "vocals", edit: region("gain", 0, 513 / 8000) });
        const atLimit = await workerState(client), recipe128 = await client.call("recovery-snapshot");
        assert(recipe128.edits.length === 128, "Recovery must accept exactly 128 edits");
        const limitClient = timedClient(StudioClient);
        try {
            await limitClient.call("init");
            await mutate(limitClient, "recover", { recipe: recipe128 });
            sameState(await workerState(limitClient), atLimit, "128-edit recovery");
        } finally { limitClient.stop(); }
        await mutate(client, "region", { kind: "vocals", edit: region("reverse", 0, 513 / 8000) });
        await rejects(() => client.call("recovery-snapshot"), "129 edits must report recovery overflow");
        assert((await client.call("working")).bytes.length > 0, "Recipe overflow disabled editing/export");
        checks.push("128-edit recovery exact replay; edit 129 stays usable while recovery reports its existing limit");

        // Return real exports for extraction by Python, outside the browser's own ZIP/WAV code.
        await load(client);
        const originalBank = await client.call("stem-export"), edit = region("reverse");
        await mutate(client, "region", { kind: "vocals", edit });
        const bank = await client.call("stem-export"), info = (await client.call("init")).info;
        const audio = (await client.call("export", { format: "wav32f" })).bytes;
        const sibling = name => { const url = new URL(name, clientUrl); url.search = new URL(clientUrl).search; return url.href; };
        const { stemZip } = await import(sibling("./studio-zip.js"));
        const { buildStudioBundle } = await import(sibling("./session-export.js"));
        return { raw: base64(stemZip(bank.files, bank.manifest)),
            bundle: base64(new Uint8Array(await buildStudioBundle({ ...bank, info, audio: { name: "mix.wav", bytes: audio } }).arrayBuffer())),
            original: Object.fromEntries(originalBank.files.map(f => [f.name, base64(f.bytes)])),
            preview: base64(audio), edit };
    } finally { client.stop(); }
}
export async function verifyRegionEdits({ engineUrl, clientUrl }) {
    const checks = window.regionEditChecks = [];
    const engine = await import(engineUrl);
    await engine.default();
    await directChecks(engine, checks);
    const { StudioClient } = await import(clientUrl);
    const artifacts = await workerChecks(StudioClient, clientUrl, checks);
    checks.push("edited worker exports packaged by production raw ZIP/session bundle for independent Python extraction");
    return { checks, artifacts };
}
