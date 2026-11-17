const BLOCK = 2048, QUEUE = 3;

// A persistent transport facade. The normal worker owns every DSP operation;
// the worklet only drains bounded, transferable PCM blocks.
export function createArrangementPreview(client, { beforePlay = () => {}, onError = () => {} } = {}) {
    let context, node, gain, layout, info, snapshot, playing = false, side = "after", frame = 0;
    let generation = 0, operation = 0, queued = 0, head = 0, exhausted = false, pumping = null, disposed = false;
    let clockTime = 0, clockFrame = 0, volume = 0.7, ready;
    const listeners = new Set(), recycled = [];
    const limit = () => info?.frames ?? 0;
    const error = value => { playing = false; node?.port.postMessage({ kind: "run", generation, running: false }); emit(value); onError(value); };
    function positionFrame() {
        if (!playing || !context) return frame;
        return Math.min(limit(), head, Math.max(frame, clockFrame + Math.floor((context.currentTime - clockTime) * layout.sample_rate)));
    }
    function emit(failure) {
        const state = { identity: snapshot?.preview.identity, prepared_revision: info?.prepared_revision,
            playing, side, position_frame: positionFrame(), duration_frames: snapshot?.preview.frames ?? 0,
            side_duration_frames: limit(), ...(failure ? { error: failure.message || String(failure) } : {}) };
        for (const listener of listeners) listener(state);
    }
    async function audio() {
        if (ready) return ready;
        ready = (async () => {
            const Constructor = globalThis.AudioContext || globalThis.webkitAudioContext;
            if (!Constructor || !globalThis.AudioWorkletNode) throw new Error("Arrangement playback requires Web Audio and AudioWorklet");
            context = new Constructor({ sampleRate: layout.sample_rate });
            if (context.sampleRate !== layout.sample_rate) {
                await context.close(); context = null;
                throw new Error("This device cannot play the project sample rate. Conform the project explicitly before playback.");
            }
            await context.audioWorklet.addModule(new URL("./audio/arrangement-sink.js", import.meta.url));
            node = new AudioWorkletNode(context, "arrangement-sink", { numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [layout.channels] });
            gain = context.createGain(); gain.gain.value = volume;
            node.connect(gain); gain.connect(context.destination);
            node.port.onmessage = ({ data: m }) => {
                if (m.generation !== generation) return;
                if (Number.isSafeInteger(m.frame)) { frame = m.frame; clockFrame = frame; clockTime = context.currentTime; }
                if (m.kind === "consumed") {
                    queued = Math.max(0, queued - 1); recycled.push(m.buffer);
                    if (playing && !exhausted) void pump().catch(error);
                } else if (m.kind === "ended") { playing = false; frame = limit(); emit(); }
            };
            context.addEventListener("statechange", () => {
                if (["suspended", "interrupted"].includes(context.state) && playing) facade.pause();
            });
            reset(frame);
        })().catch(value => { ready = null; throw value; });
        return ready;
    }
    function reset(nextFrame) {
        generation++; queued = 0; exhausted = false; frame = nextFrame; head = nextFrame;
        clockFrame = frame; clockTime = context?.currentTime || 0;
        node?.port.postMessage({ kind: "reset", generation, frame });
    }
    async function pump() {
        if (pumping) { await pumping; if (queued < QUEUE && !exhausted && playing) return pump(); return; }
        const token = generation;
        const work = (async () => {
            while (!disposed && token === generation && queued < QUEUE && !exhausted) {
                const buffer = recycled.pop();
                const block = await client.request("render-block", { buffer }, { transfer: buffer ? [buffer] : [] });
                if (token !== generation || disposed) { recycled.push(block.buffer); return; }
                const count = Math.min(block.frames, Math.max(0, limit() - head));
                const final = head + count >= limit() || count < BLOCK;
                head += count; exhausted = final;
                if (!count) { recycled.push(block.buffer); if (!queued) { playing = false; frame = limit(); emit(); } break; }
                queued++;
                node.port.postMessage({ kind: "block", generation: token, buffer: block.buffer,
                    frames: count, channels: layout.channels, final }, [block.buffer]);
            }
        })();
        pumping = work;
        try { await work; } finally { if (pumping === work) pumping = null; }
    }
    async function prepare(frameValue, sideValue, token) {
        // Await an outstanding block before changing the engine's playback head.
        if (pumping) await pumping;
        const next = await client.request("prepare", { expectedRevision: snapshot.revision, side: sideValue, frame: frameValue });
        if (token !== operation || disposed) return false;
        info = next; side = sideValue; reset(next.position_frame); return true;
    }
    async function startBuffered(token) {
        await pump();
        if (token !== operation || disposed || !queued) return;
        playing = true; clockTime = context.currentTime; clockFrame = frame;
        node.port.postMessage({ kind: "run", generation, running: true }); emit();
    }
    const facade = {
        kind: "arrangement",
        get identity() { return snapshot?.preview.identity; },
        get before_identity() { return snapshot ? `${snapshot.epoch}:${snapshot.revision}:before` : null; },
        get after_identity() { return snapshot?.preview.identity; },
        get sample_rate() { return layout?.sample_rate; }, get channels() { return layout?.channels; },
        get duration_frames() { return snapshot?.preview.frames || 0; },
        get side_duration_frames() { return limit(); },
        get playing() { return playing; }, get side() { return side; },
        position() { return positionFrame() / (layout?.sample_rate || 1); }, positionFrame,
        async play() {
            if (disposed || playing || !limit()) return;
            const token = ++operation;
            await beforePlay(); await audio(); await context.resume();
            if (token !== operation || disposed) return;
            const next = frame >= limit() ? 0 : frame;
            if (!await prepare(next, side, token)) return;
            await startBuffered(token);
        },
        pause() {
            operation++; frame = positionFrame(); playing = false;
            node?.port.postMessage({ kind: "run", generation, running: false }); emit();
        },
        stop() { operation++; playing = false; reset(0); emit(); },
        async seek(seconds) {
            if (!Number.isFinite(seconds)) throw new Error("Seek time must be finite");
            return facade.seekFrame(Math.round(seconds * layout.sample_rate));
        },
        async seekFrame(value) {
            if (!Number.isSafeInteger(value) || value < 0) throw new Error("Seek requires a nonnegative project frame");
            const resume = playing; facade.pause(); const token = ++operation;
            reset(Math.min(value, limit()));
            if (!await prepare(frame, side, token)) return;
            emit(); if (resume) await facade.play();
        },
        async choose(value) {
            if (!["before", "after"].includes(value)) throw new Error("Choose before or after");
            const resume = playing, next = positionFrame(); facade.pause(); const token = ++operation;
            const selected = await client.request("prepare", { expectedRevision: snapshot.revision, side: value, frame: 0 });
            if (token !== operation || disposed) return;
            info = selected;
            if (await prepare(Math.min(next, selected.frames), value, token)) {
                emit(); if (resume && token === operation) await facade.play();
            }
        },
        setVolume(value) {
            if (!Number.isFinite(value) || value < 0 || value > 2) throw new Error("Listening volume must be between 0 and 2");
            volume = value; if (gain) gain.gain.setTargetAtTime(value, context.currentTime, 0.005);
        },
        subscribe(listener) { listeners.add(listener); return () => listeners.delete(listener); },
        clear() {
            if (disposed) return;
            facade.stop(); disposed = true; listeners.clear(); node?.disconnect(); gain?.disconnect();
            void context?.close().catch(() => {});
        },
    };
    return {
        facade,
        async adopt(value, { signal } = {}) {
            if (signal?.aborted) throw new DOMException("Cancelled", "AbortError");
            if (disposed) throw new Error("Arrangement transport was cleared");
            snapshot = value;
            layout = { sample_rate: value.arrangement.sample_rate, channels: value.arrangement.channels };
            const current = await client.request("playback-info").catch(() => null);
            if (current?.prepared_revision === value.revision) { info = current; side = current.side; emit(); return facade; }
            const token = ++operation, resume = playing;
            const nextFrame = Math.min(positionFrame(), value.preview.frames);
            if (pumping) await pumping;
            const next = await client.request("prepare", { expectedRevision: value.revision, side: "after", frame: nextFrame });
            if (token !== operation || signal?.aborted) throw new DOMException("Superseded", "AbortError");
            info = next; side = "after";
            // Structural edits replace queue geometry atomically. Live faders
            // and fixed-scope EQ use the already-prepared branch above and
            // retain the running engine and queue without any seek/render.
            playing = false; reset(nextFrame);
            if (resume) await startBuffered(token);
            emit(); return facade;
        },
    };
}
