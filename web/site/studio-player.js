import { Player } from "./audio/player.js";
import { toBuffer } from "./dsp-client.js";
export class StudioPlayback {
    constructor(onChange) {
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
        return {
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
        this.offset = Math.min(position, this.after.duration);
        if (playing) this.play().catch((error) => this.onChange(error));
    }
    position() {
        return this.playing
            ? Math.min(
                  this.after?.duration || 0,
                  this.offset + this.context().currentTime - this.started,
              )
            : this.offset;
    }
    async play() {
        if (!this.after) return;
        const request = ++this.generation;
        const context = this.context();
        await context.resume();
        if (request !== this.generation || !this.after) return;
        this.pause();
        if (this.offset >= this.after.duration) this.offset = 0;
        const generation = ++this.generation,
            source = context.createBufferSource();
        source.buffer = this[this.side];
        source.connect(this.gain);
        source.start(0, Math.min(this.offset, source.buffer.duration));
        source.onended = () => {
            if (this.playing && generation === this.generation) {
                this.playing = false;
                this.offset = 0;
                this.onChange();
            }
        };
        this.node = source;
        this.started = context.currentTime;
        this.playing = true;
        this.onChange();
    }
    pause() {
        if (this.playing) this.offset = this.position();
        this.playing = false;
        ++this.generation;
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
    clear() {
        this.stop();
        this.before = this.after = null;
        if (this.decoder.ctx?.state === "running")
            this.decoder.ctx.suspend().catch((error) => this.onChange(error));
    }
}
