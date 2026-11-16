import { stemLevels, smoothStemGain } from "../studio-stem-mixer.js";

// Each stem is a native buffered media stream. Faders and balance are light
// AudioParams; full-resolution mixing, edits and export run on the server.
export class StreamingStemMixer {
    constructor(context, destination, bank, settings, offset, onended) {
        Object.assign(this, { context, destination, bank, settings, offset, onended });
        this.tracks = new Map(); this.nodes = []; this.playing = false;
        this.controller = new AbortController();
    }
    async start() {
        const signal = this.controller.signal;
        try {
            for (const [kind, stem] of this.bank) {
                const audio = new Audio(); audio.preload = "metadata"; audio.crossOrigin = "anonymous"; audio.src = stem.buffer.mediaUrl;
                const source = this.context.createMediaElementSource(audio);
                const splitter = this.context.createChannelSplitter(stem.peaks.length), merger = this.context.createChannelMerger(stem.peaks.length);
                const gains = stem.peaks.map(() => this.context.createGain());
                gains.forEach(g => g.gain.value = 0);
                source.connect(splitter);
                gains.forEach((gain, ch) => { splitter.connect(gain, ch); gain.connect(merger, 0, ch); });
                merger.connect(this.destination);
                this.nodes.push(source, splitter, merger, ...gains); this.tracks.set(kind, { audio, gains });
            }
            await Promise.all([...this.tracks.values()].map(({ audio }) => new Promise((resolve, reject) => {
                const clean = () => { clearTimeout(timer); audio.onloadedmetadata = audio.onerror = null; signal.removeEventListener("abort", abort); };
                const abort = () => { clean(); reject(signal.reason); };
                const timer = setTimeout(() => { clean(); reject(new Error("Stem preview timed out. Check the server connection.")); }, 30000);
                audio.onloadedmetadata = () => { clean(); resolve(); };
                audio.onerror = () => { clean(); reject(new Error("A stem preview could not load.")); };
                signal.addEventListener("abort", abort, { once: true });
                if (audio.readyState >= 1) { clean(); resolve(); }
                if (signal.aborted) abort();
            })));
            signal.throwIfAborted();
            for (const { audio } of this.tracks.values()) audio.currentTime = Math.min(this.offset, audio.duration);
            await Promise.all([...this.tracks.values()].map(({ audio }) => audio.play()));
            signal.throwIfAborted();
            const lead = this.tracks.values().next().value.audio;
            for (const { audio } of this.tracks.values()) if (Math.abs(audio.currentTime - lead.currentTime) > .04) audio.currentTime = lead.currentTime;
            lead.onended = this.onended;
            this.playing = true; this.update(this.settings, true);
        } catch (error) { this.stop(); throw error; }
    }
    position() { return this.tracks.values().next().value?.audio.currentTime ?? this.offset; }
    update(settings, immediate = false) {
        this.settings = structuredClone(settings);
        const levels = stemLevels(this.bank, settings);
        for (const [kind, { gains }] of this.tracks)
            gains.forEach((gain, ch) => smoothStemGain(gain.gain, levels.get(kind)[ch], this.context.currentTime, immediate));
    }
    stop() {
        this.controller.abort(new DOMException("Stem playback stopped", "AbortError")); this.playing = false;
        for (const { audio } of this.tracks.values()) { audio.pause(); audio.onended = null; audio.removeAttribute("src"); audio.load(); }
        for (const node of this.nodes) node.disconnect();
        this.nodes = []; this.tracks.clear();
    }
}
