import { Player } from "./player.js";
import { createLiveEqNode } from "./live-eq-node.js";
import { COMPRESSED_PREVIEW_MIME, createCompressedPreview, supportsCompressedPreview } from "./compressed-preview-source.js";

async function metadata(audio, signal) {
    if (audio.readyState >= 1) return;
    await new Promise((resolve, reject) => {
        const clean = () => { clearTimeout(timer); audio.removeEventListener("loadedmetadata", ready); audio.removeEventListener("error", fail); signal.removeEventListener("abort", abort); };
        const ready = () => { clean(); resolve(); };
        const fail = () => { clean(); reject(new Error("Server preview could not load. Check the server connection and retry playback.")); };
        const abort = () => { clean(); reject(signal.reason); };
        const timer = setTimeout(fail, 30000);
        audio.addEventListener("loadedmetadata", ready);
        audio.addEventListener("error", fail);
        signal.addEventListener("abort", abort, { once: true });
        if (signal.aborted) abort();
    });
}

// Native media buffering stays bounded independently of the song's decoded
// length. The small live-EQ worklet processes only the current audio blocks.
export class StreamingPlayer extends Player {
    constructor(onState, onError) { super(onState, onError); this.media = new Map(); }
    entry(asset) {
        if (!asset?.remote || !asset.mediaUrl) throw new Error("The server preview is unavailable.");
        if (!this.media.has(asset.id)) {
            const audio = new Audio();
            audio.preload = "metadata"; audio.crossOrigin = "anonymous";
            if (asset.previewMime && asset.previewMime !== "audio/mpeg" &&
                (asset.previewMime !== COMPRESSED_PREVIEW_MIME || !supportsCompressedPreview()))
                throw new Error("This browser cannot play the prepared server preview. Your work is kept.");
            const compressed = asset.previewMime === COMPRESSED_PREVIEW_MIME
                ? createCompressedPreview(audio, asset.mediaUrl, asset.duration, error => { this.pause(); this.onError?.(error); }) : null;
            audio.src = compressed?.url || asset.mediaUrl;
            audio.addEventListener("ended", () => { if (this.playing && this.activeEntry?.audio === audio) this.stop(); });
            this.media.set(asset.id, { audio, compressed, node: this.context().createMediaElementSource(audio) });
        }
        return this.media.get(asset.id);
    }
    releaseExcept(ids) {
        for (const [id, entry] of this.media) if (!ids.includes(id)) {
            entry.audio.pause(); entry.compressed?.dispose(); entry.audio.removeAttribute("src"); entry.audio.load(); entry.node.disconnect(); this.media.delete(id);
        }
    }
    setOriginal(audio) { super.setOriginal(audio); this.releaseExcept([audio?.id]); }
    setMaster(audio, ...args) { super.setMaster(audio, ...args); this.releaseExcept([this.original?.id, audio?.id]); }
    position() {
        return this.playing && this.activeEntry
            ? Math.max(0, Math.min(this.original?.duration || 0, this.activeEntry.audio.currentTime - (this.delay || 0))) : this.offset;
    }
    async play() {
        if (!this.original) return;
        this.onPlaybackRequest?.();
        this.pause();
        const generation = ++this.generation, operation = this.startAbort = new AbortController();
        const current = () => generation === this.generation && !operation.signal.aborted;
        try {
            const context = this.context();
            await context.resume(); if (!current()) return;
            if (this.offset >= this.original.duration - .01) this.offset = 0;
            const original = this.entry(this.original), master = this.master ? this.entry(this.master) : null;
            await Promise.all([original, master].filter(Boolean).flatMap(e => [metadata(e.audio, operation.signal), e.compressed?.ready].filter(Boolean)));
            if (!current()) return;
            this.dryGain = context.createGain(); this.wetGain = context.createGain();
            this.dryGain.gain.value = this.wetGain.gain.value = 0;
            this.nodes = [this.dryGain, this.wetGain];
            this.delay = 0;
            if (master) {
                const result = await createLiveEqNode(context, this.original.numberOfChannels, this.analysis, this.settings,
                    { signal: operation.signal, onDelta: delta => { this.delta = delta; if (this.playing) this.updateGains(); },
                        onError: error => { this.pause(); this.onError?.(error); } });
                if (!current()) { result.node.port.postMessage({ stop: true }); result.node.port.close(); result.node.disconnect(); return; }
                this.worklet = result.node; this.delay = result.delay;
                master.node.connect(this.worklet); this.worklet.connect(this.wetGain); this.wetGain.connect(this.mix);
                this.nodes.push(this.worklet);
            }
            const delay = context.createDelay(1); delay.delayTime.value = this.delay;
            original.node.connect(delay); delay.connect(this.dryGain); this.dryGain.connect(this.mix); this.nodes.push(delay);
            for (const [entry, asset] of [[original, this.original], [master, this.master]]) {
                if (!entry) continue;
                // Exact-frame streams already begin at the requested sample.
                // Even a redundant seek to zero can stall a nonseekable MP3.
                if (asset.seekable !== false) entry.audio.currentTime = Math.min(this.offset, entry.audio.duration || this.offset);
            }
            await Promise.all([original, master].filter(Boolean).map(entry => entry.audio.play()));
            if (!current()) return;
            this.activeEntry = this.source === "master" && master ? master : original;
            if (master && Math.abs(original.audio.currentTime - master.audio.currentTime) > .04)
                original.audio.currentTime = master.audio.currentTime;
            this.playing = true; this.updateGains(true); this.onState?.();
        } catch (error) {
            if (current()) { this.pause(); throw error; }
        }
    }
    pause() {
        if (this.playing) this.offset = this.position();
        this.playing = false; ++this.generation;
        this.startAbort?.abort(new DOMException("Playback stopped", "AbortError")); this.startAbort = null;
        for (const entry of this.media?.values() || []) { entry.audio.pause(); entry.node.disconnect(); }
        for (const node of this.nodes || []) node.disconnect();
        this.worklet?.port.postMessage({ stop: true }); this.worklet?.port.close();
        this.nodes = []; this.worklet = null; this.activeEntry = null;
        this.onState?.();
    }
    select(source) {
        if (this.playing) {
            const next = this.media.get((source === "master" ? this.master : this.original)?.id);
            if (next && this.activeEntry && Math.abs(next.audio.currentTime - this.activeEntry.audio.currentTime) > .04)
                next.audio.currentTime = this.activeEntry.audio.currentTime;
            this.activeEntry = next;
        }
        super.select(source);
    }
    clear() { this.stop(); this.original = this.master = null; this.releaseExcept([]); }
}
