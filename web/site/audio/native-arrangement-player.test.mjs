import test from "node:test";
import assert from "node:assert/strict";
import { NativeArrangementPlayer } from "./native-arrangement-player.js";
import { COMPRESSED_PREVIEW_MIME } from "./compressed-preview-source.js";

function deferred() {
    let resolve, reject;
    const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    return { promise, resolve, reject };
}
class AudioFixture extends EventTarget {
    constructor() {
        super(); Object.assign(this, { readyState: 0, currentTime: 0, paused: true, ended: false, volume: 1, plays: 0, loads: 0 });
    }
    play() {
        ++this.plays;
        if (this.failure) return Promise.reject(this.failure);
        this.paused = false; this.dispatchEvent(new Event("playing"));
        return this.started?.promise || Promise.resolve();
    }
    pause() { this.paused = true; this.dispatchEvent(new Event("pause")); }
    removeAttribute(name) { delete this[name]; }
    load() { ++this.loads; }
    metadata() { this.readyState = 1; this.dispatchEvent(new Event("loadedmetadata")); }
    end(time) {
        this.currentTime = time; this.ended = true; this.paused = true;
        this.dispatchEvent(new Event("pause")); this.dispatchEvent(new Event("ended"));
    }
}
const asset = (id = "stream-1", previewMime = COMPRESSED_PREVIEW_MIME) => ({
    id, previewMime, remote: true, seekable: false, mediaUrl: `/media/${id}`, duration: .144,
});
function fixture() {
    const audio = [], previews = [], errors = [], states = [];
    const player = new NativeArrangementPlayer(() => states.push(player.playing), error => errors.push(error), {
        supportsPreview: () => true,
        createAudio() { const next = new AudioFixture(); audio.push(next); return next; },
        createPreview() {
            const ready = deferred(), next = { ready: ready.promise, resolve: ready.resolve, url: "blob:owned", disposed: 0,
                dispose() { ++this.disposed; ready.reject(new DOMException("Disposed", "AbortError")); } };
            // The product reader also owns a rejection handler before playback.
            void next.ready.catch(() => {}); previews.push(next); return next;
        },
    });
    return { player, audio, previews, errors, states };
}

test("native server preview waits for its startup buffer without a Web Audio graph and retains short EOF", async () => {
    const value = fixture(), { player } = value;
    player.setOriginal(asset());
    const pending = player.play(), media = value.audio[0];
    assert.equal(media.plays, 0);
    assert.equal(player.ctx, undefined);
    media.metadata(); await new Promise(resolve => setImmediate(resolve));
    assert.equal(media.plays, 0, "Metadata alone cannot bypass the compressed startup buffer");
    value.previews[0].resolve(); await pending;
    assert.equal(player.playing, true);
    media.addEventListener("pause", () => {
        if (media.ended) assert.equal(player.playing, true, "Natural pause must wait for ended before retiring its stream");
    }, { once: true });
    media.end(.144);
    assert.equal(player.playing, false); assert.equal(player.position(), .144);
    assert.equal(value.errors.length, 0);
    player.clear(); assert.equal(value.previews[0].disposed, 1);
});

test("Stop during native buffering cannot start late audio or leave owned readers", async () => {
    const value = fixture(), { player } = value;
    player.setOriginal(asset()); const pending = player.play();
    const media = value.audio[0]; player.pause(); player.releaseExcept([]);
    media.metadata(); value.previews[0].resolve(); await pending;
    assert.equal(player.playing, false); assert.equal(media.paused, true);
    assert.equal(media.plays, 0);
    assert.equal(value.previews[0].disposed, 1); assert.equal(player.current, null);
    assert.equal(value.errors.length, 0);
});

test("a completed play promise cannot retire media before its queued ended event", async () => {
    const value = fixture(), { player } = value;
    player.setOriginal(asset()); const media = player.entry().audio;
    media.started = deferred(); const pending = player.play();
    media.metadata(); value.previews[0].resolve();
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(player.playing, true);
    media.currentTime = .144; media.ended = true; media.paused = true;
    media.started.resolve(); await pending;
    assert.equal(player.playing, true, "The queued native ended event still owns this transition");
    assert.equal(value.previews[0].disposed, 0);
    media.dispatchEvent(new Event("pause"));
    assert.equal(player.playing, true);
    media.dispatchEvent(new Event("ended"));
    assert.equal(player.playing, false); assert.equal(player.position(), .144);
    player.clear(); assert.equal(value.previews[0].disposed, 1);
});

test("a delayed obsolete native play continuation never pauses its replacement", async () => {
    const value = fixture(), { player } = value;
    player.setOriginal(asset("first")); const first = player.play();
    value.audio[0].metadata(); value.previews[0].resolve();
    player.setOriginal(asset("second")); const second = player.play();
    value.audio[1].metadata(); value.previews[1].resolve();
    await Promise.all([first, second]);
    assert.equal(player.original.id, "second"); assert.equal(player.playing, true);
    assert.equal(value.audio[1].paused, false); assert.equal(value.audio[0].paused, true);
    player.clear();
});

test("progressive MP3 uses native media with exact stream start and live listening volume", async () => {
    const value = fixture(), { player } = value;
    player.setOriginal(asset("mp3", "audio/mpeg")); const pending = player.play();
    const media = value.audio[0]; media.metadata(); await pending;
    assert.equal(value.previews.length, 0); assert.equal(media.currentTime, 0);
    assert.equal(media.src, "/media/mp3"); assert.equal(media.volume, .7);
    player.volume = .2; player.updateGains(); assert.equal(media.volume, .2);
    media.currentTime = .08; media.pause();
    assert.equal(player.playing, false); assert.equal(player.position(), .08);
    player.clear(); assert.equal(media.src, undefined);
});

test("unsupported preview, rejected playback and invalid volume fail explicitly", async () => {
    const value = fixture(), { player } = value;
    assert.throws(() => player.setOriginal(asset("bad", "audio/unknown")), /cannot play/);
    assert.equal(value.audio.length, 0);
    player.setOriginal(asset("mp3", "audio/mpeg"));
    const media = player.entry().audio;
    media.readyState = 1; media.failure = new DOMException("Gesture required", "NotAllowedError");
    await assert.rejects(player.play(), /Gesture required/);
    assert.equal(player.playing, false);
    for (const invalid of [-.1, 1.1, NaN]) {
        player.volume = invalid; assert.throws(() => player.updateGains(), /zero and one/);
    }
    player.clear();
});
