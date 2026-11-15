import { assert, equal, rejects, fixture, outside, wave } from "./doctor_pcm_checks.js";

function boundedClient(StudioClient) {
    const client = new StudioClient(), call = client.call.bind(client);
    client.call = async (action, ...args) => {
        let timer;
        try {
            return await Promise.race([call(action, ...args), new Promise((_, reject) => {
                timer = setTimeout(() => {
                    const error = new Error(`Doctor worker timed out after 30 seconds: ${action}`);
                    client.stop(error); reject(error);
                }, 30000);
            })]);
        } finally { clearTimeout(timer); }
    };
    return client;
}

export async function verifyDoctor({ engineUrl, clientUrl, playbackUrl }) {
    const checks = window.doctorChecks = [], engine = await import(engineUrl);
    await engine.default();
    assert(typeof engine.BrowserStudio.prototype.scan_issues === "function", "Build current Doctor WASM before verification");
    for (const channels of [1, 2]) {
        const studio = new engine.BrowserStudio(), input = fixture(channels);
        try {
            studio.load(input.samples, input.rate, channels);
            const original = studio.stem_wav("source"), info = studio.info(), preview = studio.current_wav32();
            const reportJson = studio.scan_issues("source"), report = JSON.parse(reportJson);
            assert(report.schema_version === 1 && report.channels === channels && report.thresholds, "Report schema/layout missing");
            assert(studio.info() === info, "Scan changed metadata/history"); equal(studio.current_wav32(), preview, "Read-only scan preview");
            assert(reportJson === studio.scan_issues("source"), "Repeated scan JSON differs");
            const clip = report.issues.find(issue => issue.kind === "clipping" && issue.repair === "declip");
            assert(clip, "Known short full-scale lobe not offered for conservative review");
            const repair = { action: clip.repair, start_seconds: clip.start_seconds, end_seconds: clip.end_seconds, strength: 0.5 };
            studio.doctor("source", JSON.stringify(repair));
            const fixed = studio.current_wav32();
            outside(wave(fixed), input.samples, repair, input.rate, channels);
            equal(studio.stem_wav("source"), original, "Immutable original after declip");
            studio.history(false); equal(studio.current_wav32(), preview, "Declip undo");
            studio.history(true); equal(studio.current_wav32(), fixed, "Declip redo");
            studio.doctor("source", JSON.stringify({ action: "reduce_low_mids", start_seconds: 0.2, end_seconds: 0.8, strength: 0.5 }));
            const reduced = wave(studio.current_wav32());
            outside(reduced, input.samples, { start_seconds: 0.2, end_seconds: 0.8 }, input.rate, channels);
            assert(reduced.some((v, i) => v !== input.samples[i]), "Low-mid repair did not change a low-mid tone");
            const retained = studio.current_wav32(), retainedInfo = studio.info();
            for (const repair of [{ action: "declip", start_seconds: -1, end_seconds: 1, strength: 0.5 },
                { action: "reduce_low_mids", start_seconds: 0, end_seconds: 1, strength: 2 }])
                await rejects(() => studio.doctor("source", JSON.stringify(repair)), "Invalid direct repair");
            equal(studio.current_wav32(), retained, "Invalid repair PCM retention"); assert(studio.info() === retainedInfo, "Invalid repair changed history");
            equal(studio.stem_wav("source"), original, "Original remains exact");
        } finally { studio.free(); }
        const clean = new engine.BrowserStudio();
        try {
            const samples = new Float32Array(48000 * channels);
            for (let i = 0; i < 48000; i++) samples[i * channels + channels - 1] = 0.25 * Math.sin(2 * Math.PI * 50 * i / 48000);
            clean.load(samples, 48000, channels);
            const report = JSON.parse(clean.scan_issues("source"));
            assert(!report.issues.some(i => i.kind === "clipping" || i.kind === "silence"), "Normalized bass / silent companion channel false positive");
        } finally { clean.free(); }
        checks.push(`WASM ${channels}ch: deterministic read-only scan, conservative clipping/quiet counterexamples, both repairs, exact original/outside-region/undo/redo, invalid-request retention`);
    }
    const { StudioClient } = await import(clientUrl), client = boundedClient(StudioClient), restored = boundedClient(StudioClient);
    const snapshot = async c => ({ info: JSON.stringify((await c.call("init")).info),
        preview: (await c.call("working")).bytes, files: (await c.call("stem-export")).files });
    const same = (a, b, label, metadata = true) => {
        if (metadata) assert(a.info === b.info, `${label}: info/history`);
        equal(a.preview, b.preview, `${label}: preview`);
        for (const file of a.files) equal(file.bytes, b.files.find(f => f.name === file.name).bytes, `${label}: ${file.name}`);
    };
    const commit = async (c, action, values) => {
        const next = await c.call(action, values); await c.call("commit", { proposal: next.proposal }); return next;
    };
    try {
        await client.call("init");
        const input = fixture(), other = fixture(2, false);
        const stems = [
            { ...input, kind: "vocals", display_name: "Doctor fixture vocals" },
            { ...other, kind: "instrumental" }];
        await commit(client, "stems", { stems });
        const before = await snapshot(client), recipeBefore = await client.call("recovery-snapshot");
        const report = await client.call("doctor-scan", { target: "vocals" });
        same(await snapshot(client), before, "Worker read-only scan");
        assert((await client.call("recovery-snapshot")).edits.length === recipeBefore.edits.length, "Scan entered recovery recipe");
        const clip = report.issues.find(i => i.repair === "declip"); assert(clip, "Worker clip missing");
        const repair = { action: "reduce_low_mids", start_seconds: 0.2, end_seconds: 0.8, strength: 0.5 };
        const values = { target: "vocals", repair };
        const proposal = await client.call("doctor", values);
        same(await snapshot(client), before, "Uncommitted Doctor proposal");
        equal(proposal.before, input.samples, "Direct stem Before exact PCM");
        assert(proposal.info.direct_edit_stems.includes("vocals") && !proposal.info.preview_is_mix, "Doctor direct-edit source handling");
        await rejects(() => client.call("commit", { proposal: proposal.proposal + 999 }), "Stale commit");
        await rejects(() => client.call("doctor", values), "Overlapping proposal");
        await client.call("discard", { proposal: proposal.proposal });
        same(await snapshot(client), before, "Discarded Doctor proposal");
        assert((await client.call("recovery-snapshot")).edits.length === 0, "Discard entered recipe");
        await rejects(() => client.call("doctor", { ...values, repair: { ...repair, strength: -1 } }), "Invalid worker request");
        same(await snapshot(client), before, "Invalid worker request");
        const fixed = await commit(client, "doctor", values), fixedState = await snapshot(client);
        outside(fixed.samples, input.samples, repair, input.rate, input.channels);
        for (const file of fixedState.files) equal(file.bytes, before.files.find(f => f.name === file.name).bytes, "Preview-only bank retained");
        await commit(client, "history", { redo: false }); same(await snapshot(client), before, "Worker undo", false);
        await commit(client, "history", { redo: true }); same(await snapshot(client), fixedState, "Worker redo");
        await commit(client, "replace", { kind: "vocals" });
        const replaced = await snapshot(client);
        equal(replaced.files.find(f => f.name === "instrumental.wav").bytes, before.files.find(f => f.name === "instrumental.wav").bytes, "Other stem retained");
        const recipe = await client.call("recovery-snapshot");
        assert(recipe.edits.some(e => e.action === "doctor") && !recipe.edits.some(e => e.action === "doctor-scan"), "Recovery action scope");
        assert(!recipe.edits.some(e => "report" in e), "Report snapshot persisted as editable recovery state");
        await restored.call("init"); await commit(restored, "recover", { recipe: structuredClone(recipe) });
        same(await snapshot(restored), replaced, "Fresh-worker recovery");
        await commit(restored, "history", { redo: false });
        await commit(client, "history", { redo: false }); same(await snapshot(restored), await snapshot(client), "Recovered undo");
        await commit(restored, "history", { redo: true });
        await commit(client, "history", { redo: true }); same(await snapshot(restored), await snapshot(client), "Recovered redo");
        const snapshotOrigin = await client.call("recovery-snapshot");
        equal(new Float32Array(await snapshotOrigin.stems[0].audio.data.arrayBuffer()), input.samples, "Recovery immutable original PCM");
        checks.push("Real worker: read-only recipe scope; candidate/discard/stale/error retention; preview-only stem fix, explicit bank apply, exact original/other stem; undo/redo and fresh-worker recovery");

        // A new playback consumer needs its own worker import's full bank before
        // receiving later revisions that correctly omit unchanged stem PCM.
        const transport = boundedClient(StudioClient);
        const { StudioPlayback } = await import(playbackUrl), playback = new StudioPlayback(() => {});
        try {
            await transport.call("init");
            const imported = await commit(transport, "stems", { stems });
            assert(imported.stems.every(stem => stem.samples instanceof Float32Array), "Fresh playback import must include every stem's PCM");
            playback.setAudio(playback.prepareAudio(imported), true);
            const bank = playback.stems;
            const settings = structuredClone(imported.info.controls); settings.tracks.forEach(t => { t.mute = true; });
            const muted = await commit(transport, "mix", { settings });
            assert(muted.stems.every(stem => stem.samples === undefined), "Unchanged bank should keep omitting replacement PCM");
            playback.setAudio(playback.prepareAudio(muted));
            for (const [kind, stem] of bank)
                assert(playback.stems.get(kind) === stem, "Unchanged live bank should reuse its imported buffer");
            playback.seek(0.3); await playback.play();
            const mutedFix = await commit(transport, "doctor", { target: "preview", repair });
            assert(mutedFix.samples.every(v => v === 0), "Doctor revived an all-muted preview");
            const position = playback.position();
            playback.setAudio(playback.prepareAudio(mutedFix));
            for (let i = 0; !playback.playing && i < 100; i++)
                await new Promise(resolve => setTimeout(resolve, 10));
            assert(playback.playing && playback.position() >= position - 0.01, "Doctor reset playing transport position");
            assert(!playback.monitorMix && !playback.mixer && playback.node?.buffer === playback.after, "Doctor preview routing incorrect");
            assert(playback.after.getChannelData(0).every(v => v === 0), "All-muted direct preview is audible");
            playback.pause();
        } finally { transport.stop(); playback.clear(); await playback.decoder.ctx?.close(); }
        checks.push("Real playback buffers: all-muted Doctor preview remains silent; playing position and standalone-preview routing retained");
    } finally { client.stop(); restored.stop(); }
    return { checks };
}
