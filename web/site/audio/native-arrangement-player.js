import { COMPRESSED_PREVIEW_MIME, createCompressedPreview, supportsCompressedPreview } from "./compressed-preview-source.js";

function metadata(audio, signal) {
    if (audio.readyState >= 1) return Promise.resolve();
    return new Promise((resolve, reject) => {
        const clean = () => {
            clearTimeout(timer); audio.removeEventListener("loadedmetadata", ready);
            audio.removeEventListener("error", failed); signal.removeEventListener("abort", aborted);
        };
        const ready = () => { clean(); resolve(); };
        const failed = () => { clean(); reject(new Error("The server preview could not load. Retry playback.")); };
        const aborted = () => { clean(); reject(signal.reason); };
        const timer = setTimeout(failed, 30000);
        audio.addEventListener("loadedmetadata", ready); audio.addEventListener("error", failed);
        signal.addEventListener("abort", aborted, { once: true });
        if (signal.aborted) aborted();
    });
}

// Server arrangements already contain mixing and scoped EQ. Keep their media
// clock and output together: WebKit's MediaElementAudioSourceNode can consume
// short MP4 streams without delivering their PCM to the Web Audio graph.
export class NativeArrangementPlayer {
    constructor(onState, onError, {
        createAudio = () => new Audio(), createPreview = createCompressedPreview,
        supportsPreview = supportsCompressedPreview,
    } = {}) {
        Object.assign(this, { onState, onError, createAudio, createPreview, supportsPreview,
            generation: 0, playing: false, volume: .7, maxVolume: 1, offset: 0 });
    }
    setOriginal(asset) {
        const mime = asset?.previewMime || "audio/mpeg";
        if (!asset?.remote || !asset.id || !asset.mediaUrl || !Number.isFinite(asset.duration) || asset.duration <= 0 ||
            !["audio/mpeg", COMPRESSED_PREVIEW_MIME].includes(mime) ||
            (mime === COMPRESSED_PREVIEW_MIME && !this.supportsPreview()))
            throw new Error("This browser cannot play the prepared server preview. Your work is kept.");
        this.pause(); this.releaseExcept([]);
        this.original = { ...asset, previewMime: mime }; this.offset = 0;
    }
    entry() {
        if (this.current) return this.current;
        const asset = this.original, audio = this.createAudio();
        audio.preload = "metadata"; audio.crossOrigin = "anonymous";
        const entry = { id: asset.id, audio, preview: null, failed: false };
        const failed = error => {
            if (this.current !== entry || entry.failed) return;
            entry.failed = true; this.pause(); this.onError?.(error);
        };
        if (asset.previewMime === COMPRESSED_PREVIEW_MIME)
            entry.preview = this.createPreview(audio, asset.mediaUrl, asset.duration, failed);
        audio.addEventListener("error", () => failed(new Error("The server preview could not play. Retry playback.")));
        audio.addEventListener("pause", () => {
            if (this.playing && this.current === entry && !audio.ended) {
                this.offset = audio.currentTime; this.playing = false; this.onState?.();
            }
        });
        audio.addEventListener("ended", () => {
            if (this.current === entry) {
                this.offset = asset.duration; this.playing = false; this.onState?.();
            }
        });
        this.current = entry; audio.src = entry.preview?.url || asset.mediaUrl;
        return entry;
    }
    async play() {
        if (!this.original || this.playing) return;
        this.pause();
        const generation = ++this.generation, abort = this.startAbort = new AbortController();
        const entry = this.entry();
        const current = () => generation === this.generation && this.current === entry && !abort.signal.aborted;
        const started = () => {
            if (current()) { this.activeEntry = entry; this.playing = true; this.onState?.(); }
        };
        entry.audio.addEventListener("playing", started);
        try {
            this.updateGains(); this.activeEntry = entry;
            // Request playback before waiting for buffers; native media preserves
            // that request while loading. Never seek an exact-frame server stream.
            const loaded = metadata(entry.audio, abort.signal);
            let playing;
            try { playing = entry.audio.play(); } catch (error) { playing = Promise.reject(error); }
            await Promise.all([loaded, entry.preview?.ready, playing]);
            if (!current()) { entry.audio.pause(); return; }
            // `ended`/`pause` properties can change before their queued events.
            // Publishing stopped here would retire the media before `ended`
            // saves its final offset. Native events own stop/EOF transitions.
            if (!entry.audio.ended && !entry.audio.paused && !this.playing) {
                this.playing = true; this.onState?.();
            }
        } catch (error) {
            if (current()) { this.pause(); throw error; }
        } finally { entry.audio.removeEventListener("playing", started); }
    }
    position() { return this.playing && this.current ? this.current.audio.currentTime : this.offset; }
    pause() {
        this.offset = this.position(); this.playing = false; ++this.generation;
        this.startAbort?.abort(new DOMException("Playback stopped", "AbortError")); this.startAbort = null;
        this.current?.audio.pause(); this.onState?.();
    }
    updateGains() {
        if (!Number.isFinite(this.volume) || this.volume < 0 || this.volume > this.maxVolume)
            throw new Error("Listening volume must be between zero and one.");
        if (this.current) this.current.audio.volume = this.volume;
    }
    releaseExcept(ids) {
        const entry = this.current;
        if (!entry || ids.includes(entry.id)) return;
        this.current = this.activeEntry = null; ++this.generation;
        this.startAbort?.abort(new DOMException("Server preview retired", "AbortError")); this.startAbort = null;
        entry.preview?.dispose(); entry.audio.pause();
        entry.audio.removeAttribute("src"); entry.audio.load();
    }
    clear() { this.pause(); this.releaseExcept([]); this.original = null; this.offset = 0; }
}
