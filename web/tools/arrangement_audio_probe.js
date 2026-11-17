async () => {
    const assert = (condition, message) => { if (!condition) throw new Error(message); };
    const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
    const OriginalNode = AudioWorkletNode;
    let sink, context, chunks = [];
    globalThis.AudioWorkletNode = class extends OriginalNode {
        constructor(ctx, name, options) { super(ctx, name, options); if (name === "arrangement-sink") { sink = this; context = ctx; } }
    };
    const { createArrangementAdapter } = await import("./arrangement-adapter.js");
    const changes = [], adapter = createArrangementAdapter({ onChange: value => changes.push(value.revision) });
    function wav(rate, frames, frequency, amplitude) {
        const bytes = new Uint8Array(44 + frames * 2), view = new DataView(bytes.buffer);
        const text = (at, value) => [...value].forEach((char, i) => bytes[at + i] = char.charCodeAt(0));
        text(0, "RIFF"); view.setUint32(4, bytes.length - 8, true); text(8, "WAVE"); text(12, "fmt ");
        view.setUint32(16, 16, true); view.setUint16(20, 1, true); view.setUint16(22, 1, true);
        view.setUint32(24, rate, true); view.setUint32(28, rate * 2, true); view.setUint16(32, 2, true); view.setUint16(34, 16, true);
        text(36, "data"); view.setUint32(40, frames * 2, true);
        for (let i = 0; i < frames; i++) view.setInt16(44 + i * 2, Math.round(32767 * amplitude * Math.sin(2 * Math.PI * frequency * i / rate)), true);
        return bytes;
    }
    async function rms() {
        chunks = []; await sleep(260);
        const values = chunks.flatMap(chunk => Array.from(chunk));
        assert(values.length > 4096, "Actual AudioWorklet PCM capture is missing");
        assert(values.every(Number.isFinite), "Nonfinite worklet output");
        return Math.sqrt(values.reduce((sum, value) => sum + value * value, 0) / values.length);
    }
    try {
        let state = await adapter.read();
        assert(state.defaults.bands.length === 7, "Shared EQ defaults missing");
        const source = await adapter.previewSource({ expectedRevision: 0 });
        assert(source.duration_frames === 0 && !source.playing, "Empty real transport did not prepare");
        const review = await adapter.reviewImport([
            new File([wav(48000, 48000 * 12, 220, 0.1)], "Repeated.wav"),
            new File([wav(48000, 48000 * 8, 880, 0.05)], "Repeated.wav"),
        ]);
        assert(review.parts.length === 2, "Repeated labels lost logical source identity");
        state = await adapter.addImport(review, review.parts.map(part => ({ part_id: part.id, version_id: part.versions[0].id })), { expectedRevision: 0 });
        assert(state.arrangement.lanes.length === 2 && state.assets.length === 2, "Import did not create one lane per part");
        assert(new Set(state.assets.map(asset => asset.frames)).size === 2, "Different durations were aligned or trimmed");
        await adapter.previewSource({ expectedRevision: state.revision });
        await source.choose("before");
        assert(source.side_duration_frames === 0 && source.duration_frames === 12 * 48000, "Paused empty A reported B geometry");
        await source.play(); assert(!source.playing, "Empty A started playback");
        await source.choose("after");
        const firstClip = state.arrangement.lanes[0].clips[0];
        state = await adapter.edit({ kind: "move_clip", clip_id: firstClip.id, lane_id: state.arrangement.lanes[0].id, start_frame: 4800 }, { expectedRevision: state.revision });
        await adapter.previewSource({ expectedRevision: state.revision });
        assert(!source.playing && source.duration_frames === 12 * 48000 + 4800 && source.side_duration_frames === source.duration_frames, "Paused structural preparation reported old duration");
        state = await adapter.undo({ expectedRevision: state.revision });
        await adapter.previewSource({ expectedRevision: state.revision });
        assert(source.duration_frames === 12 * 48000 && source.side_duration_frames === source.duration_frames, "Paused Undo did not adopt restored geometry");
        const lane = state.arrangement.lanes[0], other = state.arrangement.lanes[1];
        const sourceId = lane.clips[0].source_id;
        const original = await adapter.assetWav(sourceId);
        state = await adapter.edit({ kind: "set_lane", lane_id: other.id, mute: true }, { expectedRevision: state.revision });
        const prepared = await adapter.previewSource({ expectedRevision: state.revision });
        assert(prepared === source, "A metadata edit replaced the persistent facade");
        const envelope = await adapter.waveform(sourceId, { start_frame: 0, end_frame: 48000, bins: 120 });
        assert(envelope.peaks.length === 120 && envelope.peaks.some(pair => pair[1] > 0.09), "Source waveform missing");
        let stale = false;
        try { await adapter.edit({ kind: "set_lane", lane_id: lane.id, mute: true }, { expectedRevision: 0 }); } catch (_) { stale = true; }
        assert(stale, "Stale metadata transaction was accepted");
        await source.play();
        await context.audioWorklet.addModule("./arrangement_audio_capture.js");
        const capture = new OriginalNode(context, "arrangement-capture", { numberOfInputs: 1, numberOfOutputs: 1, outputChannelCount: [2] });
        capture.port.onmessage = ({ data }) => chunks.push(data);
        sink.connect(capture); capture.connect(context.destination);
        await sleep(180); const baseline = await rms();
        assert(baseline > 0.06 && baseline < 0.08, `Incorrect real PCM level ${baseline}`);
        const beforeFrame = source.positionFrame();
        state = await adapter.edit({ kind: "set_lane", lane_id: lane.id, gain_db: -6 }, { expectedRevision: state.revision });
        await adapter.previewSource({ expectedRevision: state.revision });
        assert(source.playing && source.positionFrame() >= beforeFrame, "Live gain restarted or paused playback");
        await sleep(200); const quiet = await rms();
        assert(quiet / baseline > 0.47 && quiet / baseline < 0.54, `Live core gain ratio ${quiet / baseline}`);
        const identity = source.identity, duration = source.duration_frames;
        await source.choose("before"); assert(source.playing, "Running A/B did not preserve running state");
        assert(source.identity === identity && source.duration_frames === duration, "A/B changed B revision identity/layout");
        await sleep(200); const before = await rms();
        assert(before / baseline > 0.95 && before / baseline < 1.05, "A is not before last edit");
        source.pause(); await source.choose("after"); assert(!source.playing, "Paused A/B autostarted");
        await source.seekFrame(48000); assert(source.positionFrame() === 48000, "Exact frame seek failed");
        await source.play(); source.stop(); await sleep(180); const silence = await rms();
        assert(silence === 0 && !source.playing && source.positionFrame() === 0, "Stop did not stay silent");
        const after = await adapter.assetWav(sourceId);
        assert(original.length === after.length && original.every((byte, i) => byte === after[i]), "Source PCM changed after live editing");
        const gainRevision = state.revision;
        state = await adapter.undo({ expectedRevision: state.revision });
        assert(state.arrangement.lanes[0].gain_db === 0 && state.history.can_redo, "Undo did not restore metadata");
        state = await adapter.redo({ expectedRevision: state.revision });
        assert(state.arrangement.lanes[0].gain_db === -6 && state.revision > gainRevision, "Redo did not restore gain");
        const exported = await adapter.exportAudio({ expectedRevision: state.revision, format: "wav32f" });
        assert(exported.length > 12 * 48000 * 8, "Explicit full precision WAV render is missing");
        const zip = new File([await (await fetch("./single-source.zip")).arrayBuffer()], "Single.zip");
        const zipReview = await adapter.reviewImport([zip]);
        assert(zipReview.parts.length === 1, "Single-source ZIP inherited aligned-bank part-count rules");
        const beforeLanes = state.arrangement.lanes.length;
        state = await adapter.addImport(zipReview, [{ part_id: zipReview.parts[0].id, version_id: zipReview.parts[0].versions[0].id }], { expectedRevision: state.revision });
        assert(state.arrangement.lanes.length === beforeLanes + 1 && state.assets.some(asset => asset.frames === 4800), "ZIP source did not append an unequal-duration lane");
        let unsafe = false;
        try { await adapter.reviewImport([new File([await (await fetch("./unsafe-source.zip")).arrayBuffer()], "Unsafe.zip")]); } catch (_) { unsafe = true; }
        assert(unsafe, "Unsafe ZIP audio path was accepted");
        const effect = { id: "saved-scoped-eq", scope: { kind: "clip", clip_id: firstClip.id },
            range: { start_frame: 2000, end_frame: 4000 }, bands: state.defaults.bands, output_gain_db: -3, bypass: false };
        state = await adapter.edit({ kind: "set_eq", effect }, { expectedRevision: state.revision });
        const documentJson = JSON.stringify(state.arrangement), assetIds = state.assets.map(asset => asset.id).join();
        const saved = await adapter.saveProject({ expectedRevision: state.revision });
        assert(saved.bytes instanceof Uint8Array && saved.type === "application/zip", "Canonical project ZIP missing");
        const bad = saved.bytes.slice(); bad[70] ^= 1;
        let corrupt = false;
        try { await adapter.openProject(bad, { expectedRevision: state.revision }); } catch (_) { corrupt = true; }
        const preserved = await adapter.read();
        assert(corrupt && preserved.revision === state.revision && JSON.stringify(preserved.arrangement) === documentJson, "Invalid project changed the existing workspace");
        state = await adapter.edit({ kind: "delete_clip", clip_id: firstClip.id }, { expectedRevision: state.revision });
        state = await adapter.openProject(saved.bytes, { expectedRevision: state.revision });
        assert(JSON.stringify(state.arrangement) === documentJson && state.assets.map(asset => asset.id).join() === assetIds, "Project restore changed stable source/effect identities or coordinates");
        assert(state.history.can_undo && state.arrangement.eq[0].range.start_frame === 2000, "Project history or source-frame EQ was lost");
        const mp3 = await adapter.exportAudio({ expectedRevision: state.revision, format: "mp3" });
        assert(mp3 instanceof Uint8Array && mp3.length > 10000, "Arrangement MP3 delivery is missing");
        const verificationContext = new OfflineAudioContext(2, 1, 48000);
        const decodedMp3 = await verificationContext.decodeAudioData(mp3.slice().buffer);
        assert(decodedMp3.numberOfChannels === 2 && decodedMp3.sampleRate === 48000 &&
            decodedMp3.length >= 12 * 48000 - 1 && decodedMp3.length <= 12 * 48000 + 12000, "MP3 changed rate/channel/duration geometry");
        const mp3Channel = decodedMp3.getChannelData(0);
        const mp3Rms = Math.sqrt(mp3Channel.reduce((sum, value) => sum + value * value, 0) / mp3Channel.length);
        assert(mp3Rms > 0.03 && mp3Rms < 0.04, "MP3 does not contain the applied arrangement gain");
        assert(JSON.stringify((await adapter.read()).arrangement) === documentJson, "MP3 delivery mutated or re-mastered the arrangement");
        const fresh = createArrangementAdapter();
        try {
            await fresh.read();
            const reopened = await fresh.openProject(new File([saved.bytes], "Project.zip"), { expectedRevision: 0 });
            assert(JSON.stringify(reopened.arrangement) === documentJson, "Fresh worker restart recovery did not restore project metadata");
            const reopenedBytes = await fresh.assetWav(sourceId);
            assert(original.every((byte, i) => byte === reopenedBytes[i]), "Fresh project load changed exact immutable source PCM");
            const undone = await fresh.undo({ expectedRevision: reopened.revision });
            assert(!undone.arrangement.eq.length, "Restored project Undo did not restore the previous document");
        } finally { fresh.clear(); }
        return { passed: true, checks: ["empty real prepared transport", "single/repeated-label source ownership and unequal duration lanes", "atomic stale rejection", "actual worker/worklet PCM", "live gain without song rendering/restart", "running and paused A/B", "stable B identity/layout", "frame seek", "Stop silence", "immutable sources", "undo/redo", "explicit Float32 WAV export"],
            baseline_rms: baseline, live_gain_ratio: quiet / baseline, before_ratio: before / baseline, stop_rms: silence, metadata_events: changes.length, zip_single_source: true, zip_unsafe_rejected: true,
            project_roundtrip: true, project_fresh_worker: true, project_corrupt_preserved: true, project_restored_undo: true,
            mp3_verified_delivery: true, mp3_rms: mp3Rms, mp3_frames: decodedMp3.length };
    } finally { adapter.clear(); globalThis.AudioWorkletNode = OriginalNode; }
}
