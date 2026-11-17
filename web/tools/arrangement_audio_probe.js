async () => {
    const assert = (condition, message) => { if (!condition) throw new Error(message); };
    const OriginalNode = AudioWorkletNode;
    const options = globalThis.arrangementAudioProbeOptions || {};
    const diagnostics = globalThis.arrangementAudioProbeDiagnostics = {
        phase: "setup", waits: [], context_states: [], sink_messages: {}, errors: [], capture_blocks: 0,
    };
    let sink, context, capture, source, pending, header, nextWindow = 0;
    const snapshot = () => ({ state: context?.state, clock: context?.currentTime,
        rate: context?.sampleRate, playing: source?.playing, side: source?.side,
        position_frame: source?.positionFrame(), sink_frame: diagnostics.sink_frame,
        capture_blocks: diagnostics.capture_blocks, capture_status: diagnostics.capture_status });
    function failure(message) {
        diagnostics.errors.push(message);
        pending?.finish(new Error(message));
    }
    globalThis.AudioWorkletNode = class extends OriginalNode {
        constructor(ctx, name, settings) {
            super(ctx, name, settings);
            if (name === "arrangement-sink") {
                sink = this; context = ctx;
                ctx.addEventListener("statechange", () => diagnostics.context_states.push({ state: ctx.state, clock: ctx.currentTime }));
                this.addEventListener("processorerror", () => failure("Arrangement sink processor failed"));
                this.port.addEventListener("message", ({ data }) => {
                    diagnostics.sink_messages[data.kind] = (diagnostics.sink_messages[data.kind] || 0) + 1;
                    diagnostics.sink_frame = data.frame;
                });
            }
        }
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
    function receive({ data }) {
        if (!(data instanceof Float32Array)) {
            diagnostics.capture_status = data;
            if (data.kind === "pcm") header = data;
            return;
        }
        diagnostics.capture_blocks++;
        if (!pending || header?.window !== pending.id) return;
        if (header.first_frame < pending.firstFrame) return;
        if (header.rate !== context.sampleRate || header.channels !== 2 ||
            data.length !== (header.end_frame - header.first_frame) * 2 ||
            (pending.lastFrame !== null && pending.lastFrame !== header.first_frame)) {
            failure("Invalid or discontinuous actual worklet capture geometry"); return;
        }
        if (!data.every(Number.isFinite)) { failure("Nonfinite worklet output"); return; }
        pending.lastFrame = header.end_frame;
        pending.blocks.push({ ...header });
        pending.chunks.push(data); pending.samples += data.length;
        // Three complete 4096-frame blocks, independent of host scheduling.
        if (pending.samples >= 3 * 8192) pending.finish();
    }
    async function rms(label) {
        diagnostics.phase = label;
        assert(!diagnostics.errors.length, diagnostics.errors.join("; "));
        const id = ++nextWindow, start = performance.now(), clock = context.currentTime;
        // Allow the documented bounded pre-edit queue to drain, then measure a
        // fixed window. Never choose samples by their level or retry a dropout.
        const settleFrames = Math.max(Math.ceil(context.sampleRate * 0.2), source.max_buffered_frames + 128);
        const firstFrame = Math.ceil(clock * context.sampleRate) + settleFrames;
        const detail = { label, start: snapshot(), not_before_frame: firstFrame, settle_frames: settleFrames, max_buffered_frames: source.max_buffered_frames };
        diagnostics.waits.push(detail);
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => pending?.finish(new Error(
                `Actual AudioWorklet PCM capture is missing before bounded deadline: ${JSON.stringify(snapshot())}`
            )), options.pcmTimeoutMs || 10000);
            pending = { id, firstFrame, lastFrame: null, blocks: [], chunks: [], samples: 0,
                finish(error) {
                    clearTimeout(timer);
                    const completed = pending; pending = null;
                    detail.wall_ms = performance.now() - start; detail.end = snapshot();
                    detail.samples = completed.samples; detail.last_frame = completed.lastFrame;
                    const values = completed.chunks.flatMap(chunk => Array.from(chunk));
                    detail.raw_pcm = values;
                    detail.blocks = completed.blocks;
                    detail.zero_runs = [];
                    let zeroStart = null;
                    for (let frame = 0; frame <= values.length / 2; frame++) {
                        const silent = frame < values.length / 2 && values[frame * 2] === 0 && values[frame * 2 + 1] === 0;
                        if (silent && zeroStart === null) zeroStart = frame;
                        if (!silent && zeroStart !== null) {
                            if (frame - zeroStart > 1) detail.zero_runs.push({ offset_frame: zeroStart, frames: frame - zeroStart });
                            zeroStart = null;
                        }
                    }
                    if (error) { detail.error = error.message; reject(error); return; }
                    detail.rms = Math.sqrt(values.reduce((sum, value) => sum + value * value, 0) / values.length);
                    if (label !== "stop" && detail.zero_runs.length) {
                        reject(new Error(`Unexpected zero-output gap in ${label}: ${JSON.stringify(detail.zero_runs)}`)); return;
                    }
                    resolve(detail.rms);
                } };
            capture.port.postMessage({ kind: "measure", id, notBeforeFrame: firstFrame });
        });
    }
    try {
        let state = await adapter.read();
        assert(state.defaults.bands.length === 7, "Shared EQ defaults missing");
        source = await adapter.previewSource({ expectedRevision: 0 });
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
        capture = new OriginalNode(context, "arrangement-capture", { numberOfInputs: 1, numberOfOutputs: 1, outputChannelCount: [2] });
        capture.port.onmessage = receive;
        capture.port.onmessageerror = () => failure("Arrangement capture message failed");
        capture.addEventListener("processorerror", () => failure("Arrangement capture processor failed"));
        sink.connect(capture); capture.connect(context.destination);
        const baseline = await rms("baseline");
        assert(baseline > 0.06 && baseline < 0.08, `Incorrect real PCM level ${baseline}`);
        const beforeFrame = source.positionFrame(), beforeSinkFrame = diagnostics.sink_frame;
        const beforeResets = diagnostics.sink_messages.reset;
        state = await adapter.edit({ kind: "set_lane", lane_id: lane.id, gain_db: -6 }, { expectedRevision: state.revision });
        await adapter.previewSource({ expectedRevision: state.revision });
        assert(source.playing, "Live gain paused playback");
        const quiet = await rms("live_gain");
        // UI positions extrapolate between worklet clock messages and can be
        // corrected backwards by a quantum. Observe a full fresh PCM window and
        // require the actual sink to advance without any queue/seek reset.
        assert(source.playing && source.positionFrame() >= beforeFrame &&
            diagnostics.sink_frame >= beforeSinkFrame && diagnostics.sink_messages.reset === beforeResets,
            "Live gain restarted or paused playback");
        assert(quiet / baseline > 0.47 && quiet / baseline < 0.54, `Live core gain ratio ${quiet / baseline}`);
        const identity = source.identity, duration = source.duration_frames;
        await source.choose("before"); assert(source.playing, "Running A/B did not preserve running state");
        assert(source.identity === identity && source.duration_frames === duration, "A/B changed B revision identity/layout");
        const before = await rms("before");
        assert(before / baseline > 0.95 && before / baseline < 1.05, "A is not before last edit");
        source.pause(); await source.choose("after"); assert(!source.playing, "Paused A/B autostarted");
        await source.seekFrame(48000); assert(source.positionFrame() === 48000, "Exact frame seek failed");
        await source.play(); source.stop(); const silence = await rms("stop");
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
    } finally {
        diagnostics.final = snapshot();
        capture?.disconnect(); adapter.clear(); globalThis.AudioWorkletNode = OriginalNode;
    }
}
