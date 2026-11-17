import { StreamingPlayer } from "../audio/streaming-player.js";
import { arrangementStreamFrame, checkedArrangementStream } from "./arrangement-stream-contract.js";

// Native <audio> buffers compressed preview bytes. Rust owns mixing/EQ, and
// every explicit seek requests an exact project frame in a new server stream.
export function createArrangementStreamPreview({
    prepare, stop, mediaUrl, beforePlay = () => {}, onError = () => {},
    createPlayer = (changed, failed) => new StreamingPlayer(changed, failed),
}) {
    let snapshot, descriptor, side = "after", sideFrames = 0, frame = 0, operation = 0, disposed = false;
    const listeners = new Set();
    const player = createPlayer(() => emit(), error => { pause(); onError(error); });
    const positionFrame = () => player.playing && descriptor ? arrangementStreamFrame(descriptor, player.position()) : frame;
    function emit() {
        const event = { identity: snapshot?.preview.identity, prepared_revision: snapshot?.preview.prepared_revision,
            playing: player.playing, side, position_frame: positionFrame(), duration_frames: snapshot?.preview.frames || 0,
            side_duration_frames: sideFrames };
        for (const listener of listeners) listener(event);
    }
    function retire(value) {
        if (!value?.stream_id) return;
        void Promise.resolve().then(() => stop(value)).catch(error => {
            if (![404, 409].includes(error.status) && !disposed) onError(error);
        });
    }
    function silence() {
        frame = positionFrame(); player.pause(); player.releaseExcept([]);
        const previous = descriptor; descriptor = null; retire(previous);
    }
    function pause() { ++operation; silence(); emit(); }
    async function prepared(nextFrame, nextSide, token, signal) {
        const expected = snapshot;
        const raw = await prepare({ side: nextSide, start_frame: nextFrame }, { expectedRevision: expected.revision, epoch: expected.epoch, signal });
        const next = checkedArrangementStream(raw, expected, nextSide);
        if (disposed || token !== operation || snapshot !== expected || signal?.aborted) { retire(next); return false; }
        descriptor = next; side = nextSide; sideFrames = next.side_duration_frames; frame = next.start_frame;
        return true;
    }
    async function start(token) {
        if (!descriptor?.side_duration_frames || token !== operation || disposed) { emit(); return; }
        const active = descriptor;
        player.setOriginal({ id: active.stream_id, remote: true, mediaUrl: mediaUrl(active),
            duration: (active.side_duration_frames - active.start_frame) / active.sample_rate + active.preview_priming_frames / active.preview_sample_rate,
            sampleRate: active.preview_sample_rate, numberOfChannels: active.channels });
        await player.play();
        if (token !== operation || disposed) { player.pause(); player.releaseExcept([]); return; }
        player.activeEntry?.audio.addEventListener("ended", () => {
            if (descriptor === active) { frame = sideFrames; descriptor = null; retire(active); emit(); }
        }, { once: true });
        emit();
    }
    const facade = {
        kind: "arrangement",
        get identity() { return snapshot?.preview.identity; },
        get before_identity() { return snapshot ? `${snapshot.epoch}:${snapshot.revision}:before` : null; },
        get after_identity() { return snapshot?.preview.identity; },
        get sample_rate() { return snapshot?.arrangement.sample_rate; },
        get channels() { return snapshot?.arrangement.channels; },
        get duration_frames() { return snapshot?.preview.frames || 0; },
        get side_duration_frames() { return sideFrames; },
        get playing() { return player.playing; }, get side() { return side; },
        position: () => positionFrame() / (snapshot?.arrangement.sample_rate || 1), positionFrame,
        async play() {
            if (disposed || player.playing || !sideFrames) return;
            const next = frame >= sideFrames ? 0 : frame;
            const token = ++operation; await beforePlay();
            if (token !== operation || disposed) return;
            silence();
            if (await prepared(next, side, token)) await start(token);
        },
        pause,
        stop() { pause(); frame = 0; emit(); },
        async seek(seconds) {
            if (!Number.isFinite(seconds) || seconds < 0) throw new Error("Enter a finite nonnegative seek time.");
            return facade.seekFrame(Math.round(seconds * snapshot.arrangement.sample_rate));
        },
        async seekFrame(value) {
            if (!Number.isSafeInteger(value) || value < 0) throw new Error("Seek requires an exact nonnegative project frame.");
            const resume = player.playing; pause(); const token = ++operation;
            if (await prepared(Math.min(value, sideFrames), side, token)) { emit(); if (resume) await start(token); }
        },
        async choose(value) {
            if (!["before", "after"].includes(value)) throw new Error("Choose before or after.");
            if (value === side) return;
            const resume = player.playing, next = positionFrame(); pause(); const token = ++operation;
            if (await prepared(next, value, token)) { emit(); if (resume) await start(token); }
        },
        setVolume(value) {
            if (!Number.isFinite(value) || value < 0 || value > 2) throw new Error("Listening volume must be between zero and two.");
            player.volume = value; player.updateGains();
        },
        subscribe(listener) { listeners.add(listener); return () => listeners.delete(listener); },
        clear() { if (disposed) return; pause(); disposed = true; listeners.clear(); player.clear(); void player.ctx?.close().catch(() => {}); },
    };
    return { facade, async adopt(value, { signal } = {}) {
        if (disposed || signal?.aborted) throw new DOMException("Arrangement stream stopped", "AbortError");
        const nextFrame = positionFrame(), resume = player.playing;
        const compatible = snapshot?.epoch === value.epoch && snapshot.preview.frames === value.preview.frames &&
            snapshot.arrangement.sample_rate === value.arrangement.sample_rate && snapshot.arrangement.channels === value.arrangement.channels;
        if (resume && side === "after" && descriptor && compatible && !value.preview.pending && value.preview.prepared_revision === value.revision) {
            snapshot = value; sideFrames = value.preview.frames;
            descriptor = { ...descriptor, identity: value.preview.identity, prepared_revision: value.revision };
            emit(); return facade;
        }
        pause(); snapshot = value; const token = ++operation;
        if (!await prepared(Math.min(nextFrame, value.preview.frames), "after", token, signal)) throw new DOMException("Stream preparation superseded", "AbortError");
        emit(); if (resume) await start(token); return facade;
    } };
}
