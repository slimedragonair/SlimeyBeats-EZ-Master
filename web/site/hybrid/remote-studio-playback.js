import { StreamingPlayer } from "../audio/streaming-player.js";
import { StreamingStemMixer } from "./streaming-stem-mixer.js";

export class RemoteStudioPlayback {
    constructor(onChange, beforePlay) {
        this.onChange = onChange; this.beforePlay = beforePlay;
        const notify = error => {
            if (this.transport?.original && !this.transport.playing && !this.mixer?.playing) this.offset = this.transport.offset;
            this.onChange(error);
        };
        this.transport = new StreamingPlayer(() => notify(), notify);
        this.offset = 0; this.side = "after"; this.volume = .7; this.generation = 0;
        this.stems = new Map();
    }
    context() { return this.transport.context(); }
    get playing() { return this.transport.playing || !!this.mixer?.playing; }
    prepareAudio(result) {
        if (!result.remote || !result.beforeAudio?.remote || !result.previewAudio?.remote)
            throw new Error("The server did not return a streaming Studio preview.");
        return { before: result.beforeAudio, after: result.previewAudio, stems: result.stemAudio,
            settings: structuredClone(result.info.controls), monitorMix: result.info.preview_is_mix };
    }
    setAudio(buffers, reset = false) {
        const playing = this.playing, position = reset ? 0 : this.position();
        this.pause(); Object.assign(this, buffers); this.offset = Math.min(position, this.after.duration);
        if (playing) this.play().catch(error => this.onChange(error));
    }
    position() { return this.mixer?.playing ? Math.min(this.after.duration, this.mixer.position()) : this.transport.playing ? this.transport.position() : this.offset; }
    async play() {
        if (!this.after) return;
        this.beforePlay(); this.pause();
        const generation = ++this.generation, context = this.context();
        await context.resume(); if (generation !== this.generation) return;
        if (this.offset >= this.after.duration - .01) this.offset = 0;
        if (this.side === "after" && this.monitorMix && this.stems.size) {
            this.gain ??= context.createGain(); this.gain.gain.value = this.volume; this.gain.connect(context.destination);
            const mixer = this.mixer = new StreamingStemMixer(context, this.gain, this.stems, this.settings, this.offset, () => this.stop());
            try { await mixer.start(); }
            catch (error) { if (generation === this.generation) throw error; return; }
        } else {
            const position = this.offset;
            this.transport.setOriginal(this[this.side]); this.transport.offset = position; this.offset = position;
            this.transport.volume = this.volume; await this.transport.play();
        }
        if (generation === this.generation) this.onChange();
    }
    pause() {
        if (this.playing) this.offset = this.position();
        ++this.generation; this.transport.pause(); this.mixer?.stop(); this.mixer = null;
        this.gain?.disconnect(); this.onChange();
    }
    stop() { this.pause(); this.offset = 0; this.onChange(); }
    choose(side) { if (side !== this.side) { const playing = this.playing; this.pause(); this.side = side; if (playing) this.play().catch(error => this.onChange(error)); this.onChange(); } }
    seek(seconds) { const playing = this.playing; this.pause(); this.offset = Math.max(0, Math.min(seconds, this.after?.duration || 0)); if (playing) this.play().catch(error => this.onChange(error)); this.onChange(); }
    setVolume(value) { this.volume = value; this.transport.volume = value; this.transport.updateGains(); if (this.gain) this.gain.gain.value = value; }
    setMix(settings) {
        this.settings = structuredClone(settings); const restart = this.playing && (this.side !== "after" || !this.monitorMix);
        if (restart) this.pause(); this.side = "after"; this.monitorMix = true;
        this.mixer?.update(settings); if (restart) this.play().catch(error => this.onChange(error)); this.onChange();
    }
    clear() { this.stop(); this.transport.clear(); this.before = this.after = null; this.stems = new Map(); this.monitorMix = false; }
}
