import { Player } from "./audio/player.js";
import { toBuffer } from "./dsp-client.js";
import { StudioStemMixer, stemPeaks, MAX_STEM_BYTES } from "./studio-stem-mixer.js";
export class StudioPlayback {
    constructor(onChange, beforePlay = () => {}) {
        this.beforePlay = beforePlay;
        this.decoder = new Player(
            () => {},
            (error) => onChange(error),
        );
        this.decoder.importLimits = {
            max_file_bytes: 128 * 1024 * 1024,
            max_decoded_bytes: 128 * 1024 * 1024,
            max_seconds: 900,
        };
        this.onChange = onChange;
        this.offset = 0;
        this.side = "after";
        this.volume = 0.7;
        this.generation = 0;
    }
    context() {
        const context = this.decoder.context();
        if (!this.gain) {
            this.gain = context.createGain();
            this.gain.gain.value = this.volume;
            this.gain.connect(context.destination);
            context.addEventListener("statechange", () => {
                if (
                    ["suspended", "interrupted"].includes(context.state) &&
                    this.playing
                )
                    this.pause();
            });
        }
        return context;
    }
    decode(file) {
        return this.decoder.decode(file);
    }
    prepareAudio(result) {
        const stems = new Map();
        let bytes = 0;
        for (const stem of result.stems || []) {
            bytes += stem.length * 4;
            if (bytes > MAX_STEM_BYTES)
                throw new Error("Live stems exceed 128 MiB decoded PCM");
            const previous = this.stems?.get(stem.kind);
            if (previous?.identity === stem.identity) stems.set(stem.kind, previous);
            else {
                if (!stem.samples) throw new Error(`Missing live stem PCM: ${stem.kind}`);
                const buffer = toBuffer(this.context(), stem.samples, result.info.channels, result.info.sample_rate);
                stems.set(stem.kind, { identity: stem.identity, buffer, peaks: stemPeaks(buffer) });
            }
        }
        return {
            stems,
            settings: structuredClone(result.info.controls),
            monitorMix: result.info.preview_is_mix,
            after: toBuffer(
                this.context(),
                result.samples,
                result.info.channels,
                result.info.sample_rate,
            ),
            before: toBuffer(
                this.context(),
                result.before,
                result.info.channels,
                result.info.sample_rate,
            ),
        };
    }
    setAudio(buffers, reset = false) {
        const playing = this.playing,
            position = reset ? 0 : this.position();
        this.pause();
        this.after = buffers.after;
        this.before = buffers.before;
        this.stems = buffers.stems;
        this.settings = buffers.settings;
        this.monitorMix = buffers.monitorMix;
        this.offset = Math.min(position, this.after.duration);
        if (playing) this.play().catch((error) => this.onChange(error));
    }
    position() {
        return this.playing
            ? Math.min(
                  this.after?.duration || 0,
                  Math.max(this.offset, this.offset + this.context().currentTime - this.started),
              )
            : this.offset;
    }
    async play() {
        if (!this.after) return;
        this.beforePlay();
        const request = ++this.generation;
        const context = this.context();
        await context.resume();
        if (request !== this.generation || !this.after) return;
        this.pause();
        if (this.offset >= this.after.duration) this.offset = 0;
        const generation = ++this.generation;
        const ended = () => {
            if (this.playing && generation === this.generation) {
                this.pause();
                this.offset = 0;
                this.onChange();
            }
        };
        if (this.side === "after" && this.monitorMix && this.stems?.size) {
            this.mixer = new StudioStemMixer(context, this.gain, this.stems,
                this.settings, this.offset, ended);
        } else {
            const source = context.createBufferSource();
            source.buffer = this[this.side];
            source.connect(this.gain);
            source.onended = ended;
            source.start(0, Math.min(this.offset, source.buffer.duration));
            this.node = source;
        }
        this.started = this.mixer?.started ?? context.currentTime;
        this.playing = true;
        this.onChange();
    }
    pause() {
        if (this.playing) this.offset = this.position();
        this.playing = false;
        ++this.generation;
        this.mixer?.stop();
        this.mixer = null;
        if (this.node) {
            this.node.onended = null;
            try {
                this.node.stop();
            } catch {}
            this.node.disconnect();
            this.node = null;
        }
        this.onChange();
    }
    stop() {
        this.pause();
        this.offset = 0;
        this.onChange();
    }
    choose(side) {
        if (side === this.side) return;
        const playing = this.playing;
        this.pause();
        this.side = side;
        if (playing) this.play().catch((error) => this.onChange(error));
        this.onChange();
    }
    seek(seconds) {
        const playing = this.playing;
        this.pause();
        this.offset = Math.max(0, Math.min(seconds, this.after?.duration || 0));
        if (playing) this.play().catch((error) => this.onChange(error));
        this.onChange();
    }
    setVolume(value) {
        this.volume = value;
        if (this.gain) this.gain.gain.value = value;
    }
    setMix(settings) {
        if (!this.stems?.size) return;
        this.settings = structuredClone(settings);
        const switchPreview = (!this.monitorMix || this.side !== "after") && this.playing;
        this.monitorMix = true;
        this.side = "after";
        if (switchPreview) {
            this.pause();
            this.play().catch((error) => this.onChange(error));
        } else this.mixer?.update(this.settings);
        this.onChange();
    }
    clear() {
        this.stop();
        this.before = this.after = null;
        this.stems = null;
        this.settings = null;
        this.monitorMix = false;
        if (this.decoder.ctx?.state === "running")
            this.decoder.ctx.suspend().catch((error) => this.onChange(error));
    }
}
