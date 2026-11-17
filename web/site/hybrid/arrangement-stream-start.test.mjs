import test from "node:test";
import assert from "node:assert/strict";
import { NativeArrangementPlayer } from "../audio/native-arrangement-player.js";
import { COMPRESSED_PREVIEW_MIME } from "../audio/compressed-preview-source.js";
import { createArrangementStreamPreview } from "./arrangement-stream-preview.js";

function deferred() {
    let resolve;
    const promise = new Promise(yes => { resolve = yes; });
    return { promise, resolve };
}

class AudioFixture extends EventTarget {
    constructor(failure) {
        super();
        Object.assign(this, { failure, readyState: 1, currentTime: 0,
            paused: true, ended: false, volume: 1, loads: 0 });
    }
    play() {
        if (this.failure) return Promise.reject(this.failure);
        this.paused = false;
        this.dispatchEvent(new Event("playing"));
        return Promise.resolve();
    }
    pause() { this.paused = true; this.dispatchEvent(new Event("pause")); }
    removeAttribute(name) { delete this[name]; }
    load() { ++this.loads; }
}

// Actual facade and native player; only the browser media/reader boundary is synthetic.
function fixture(failures) {
    const snapshot = { epoch: "start-test", revision: 0,
        arrangement: { sample_rate: 48000, channels: 2 },
        preview: { identity: "start-test:0:after", frames: 96000,
            prepared_revision: 0, pending: false } };
    const media = [], readers = [], ownedReaders = new Set(), stops = [], events = [];
    let player, sequence = 0;
    const preview = createArrangementStreamPreview({
        createPlayer: (changed, failed) => player = new NativeArrangementPlayer(changed, failed, {
            supportsPreview: () => true,
            createAudio() {
                const audio = new AudioFixture(failures[media.length]);
                media.push(audio);
                return audio;
            },
            createPreview(_audio, address) {
                const id = address.split("/").at(-1);
                ownedReaders.add(id);
                const reader = { id, url: `blob:${id}`, ready: Promise.resolve(), disposals: 0,
                    dispose() { ++this.disposals; ownedReaders.delete(id); } };
                readers.push(reader);
                return reader;
            },
        }),
        mediaUrl: descriptor => descriptor.stream_path,
        stop: async descriptor => { stops.push(descriptor.stream_id); },
        prepare: async ({ side, start_frame }) => {
            const id = `s${++sequence}`;
            return { stream_id: id, stream_path: `/v2/projects/start-test/arrangement/streams/${id}`,
                epoch: snapshot.epoch, prepared_revision: snapshot.revision,
                identity: snapshot.preview.identity, sample_rate: 48000, channels: 2,
                duration_frames: snapshot.preview.frames, side_duration_frames: snapshot.preview.frames,
                start_frame, side, preview_sample_rate: 48000, preview_priming_frames: 1152,
                preview_mime: COMPRESSED_PREVIEW_MIME };
        },
    });
    preview.facade.subscribe(event => events.push(event));
    return { ...preview, snapshot, media, readers, ownedReaders, stops, events,
        get player() { return player; } };
}

const settled = () => new Promise(resolve => setImmediate(resolve));

test("a rejected initial native Play retires s2 and releases its owned reader before returning", async t => {
    const failure = new DOMException("Gesture required", "NotAllowedError");
    const h = fixture([failure]);
    t.after(() => h.facade.clear());
    await h.adopt(h.snapshot);
    await assert.rejects(h.facade.play(), error => error === failure);
    await settled();

    assert.equal(h.player.original.id, "s2");
    assert.equal(h.stops.filter(id => id === "s2").length, 1);
    assert.equal(h.ownedReaders.size, 0);
    assert.equal(h.readers[0].disposals, 1);
    assert.equal(h.player.current, null);
    assert.equal(h.player.activeEntry, null);
    assert.equal(h.media[0].paused, true);
    assert.equal(h.media[0].src, undefined);
    assert.equal(h.media[0].loads, 1);
    assert.equal(h.facade.positionFrame(), 0);
    assert.equal(h.facade.playing, false);
    assert.equal(h.facade.pending, false);
    assert.equal(h.events.at(-1).pending, false);
});

test("failed Play after a paused seek preserves the requested nonzero project frame", async t => {
    const failure = new DOMException("Playback rejected", "NotAllowedError");
    const h = fixture([failure]);
    t.after(() => h.facade.clear());
    await h.adopt(h.snapshot);
    await h.facade.seekFrame(48000);
    await assert.rejects(h.facade.play(), error => error === failure);
    await settled();

    assert.equal(h.player.original.id, "s3");
    assert.equal(h.stops.filter(id => id === "s3").length, 1);
    assert.equal(h.ownedReaders.size, 0);
    assert.equal(h.readers[0].disposals, 1);
    assert.equal(h.player.current, null);
    assert.equal(h.facade.positionFrame(), 48000);
    assert.equal(h.facade.position(), 1);
    assert.equal(h.facade.playing, false);
    assert.equal(h.facade.pending, false);
    assert.equal(h.events.at(-1).position_frame, 48000);
});

test("delivery of an obsolete native rejection cannot retire or pause its replacement", async t => {
    const failure = new DOMException("Old playback rejected", "NotAllowedError");
    const h = fixture([failure, undefined]);
    const rejected = deferred(), deliver = deferred();
    t.after(() => { deliver.resolve(); h.facade.clear(); });
    await h.adopt(h.snapshot);

    // Delay only delivery of the genuine native rejection to the facade. This
    // exercises its catch guard after a newer stream has already started.
    const nativePlay = h.player.play.bind(h.player);
    h.player.play = async () => {
        const id = h.player.original.id;
        try { await nativePlay(); }
        catch (error) {
            if (id === "s2") { rejected.resolve(); await deliver.promise; }
            throw error;
        }
    };
    const oldStart = h.facade.play();
    const oldRejected = assert.rejects(oldStart, error => error === failure);
    await rejected.promise;
    h.facade.stop();
    await h.facade.play();
    h.media[1].currentTime = .524; // Includes the descriptor's 24 ms priming.
    assert.equal(h.facade.positionFrame(), 24000);

    deliver.resolve();
    await oldRejected;
    await settled();
    assert.equal(h.player.original.id, "s3");
    assert.equal(h.player.current.id, "s3");
    assert.equal(h.media[1].paused, false);
    assert.equal(h.facade.playing, true);
    assert.equal(h.facade.pending, false);
    assert.equal(h.facade.positionFrame(), 24000);
    assert.equal(h.stops.filter(id => id === "s2").length, 1);
    assert.equal(h.stops.includes("s3"), false);
    assert.deepEqual([...h.ownedReaders], ["s3"]);
    assert.equal(h.readers[0].disposals, 1);
    assert.equal(h.readers[1].disposals, 0);
    h.facade.clear();
    await settled();
    assert.equal(h.ownedReaders.size, 0);
    assert.equal(h.stops.filter(id => id === "s3").length, 1);
});
