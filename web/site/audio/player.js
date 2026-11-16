import { validateFile, validateDecoded } from "./import-validation.js";
import { createLiveEqNode } from "./live-eq-node.js";
import { readMp3Timing, normalizeMp3Timing } from "./mp3-gapless.js";

export class Player {
    constructor(onState, onError) {
        this.onState = onState;
        this.onError = onError;
        this.offset = 0;
        this.playing = false;
        this.source = "original";
        this.volume = 0.7;
        this.match = true;
        this.generation = 0;
    }
    context() {
        if (!this.ctx) {
            const Constructor =
                globalThis.AudioContext || globalThis.webkitAudioContext;
            if (!Constructor)
                throw new Error(
                    "This browser does not support Web Audio. Try a current browser.",
                );
            this.ctx = new Constructor({ sampleRate: 48000 });
            this.mix = this.ctx.createGain();
            this.analyser = this.ctx.createAnalyser();
            this.analyser.fftSize = 4096;
            this.analyser.smoothingTimeConstant = 0.72;
            this.fader = this.ctx.createGain();
            this.fader.gain.value = this.volume;
            this.mix.connect(this.analyser);
            this.analyser.connect(this.fader);
            this.fader.connect(this.ctx.destination);
            this.ctx.addEventListener("statechange", () => {
                if (
                    this.ctx.state === "interrupted" ||
                    this.ctx.state === "suspended"
                ) {
                    if (this.playing) this.pause();
                }
            });
        }
        return this.ctx;
    }
    async decode(file) {
        validateFile(file, this.importLimits);
        const context = this.context(), encoded = await file.arrayBuffer();
        const timing = readMp3Timing(encoded);
        const decoded = await context.decodeAudioData(encoded);
        const buffer = normalizeMp3Timing(context, decoded, timing, this.importLimits.max_seconds);
        validateDecoded(buffer, this.importLimits);
        return buffer;
    }
    setOriginal(buffer) {
        this.stop();
        this.original = buffer;
        this.master = null;
        this.analysis = null;
        this.source = "original";
    }
    setMaster(buffer, analysis, settings, before, after) {
        this.pause();
        this.master = buffer;
        this.analysis = analysis;
        this.settings = structuredClone(settings);
        this.before = before;
        this.after = after;
        this.delta = 0;
    }
    position() {
        return this.playing
            ? Math.min(
                  this.original.duration,
                  Math.max(
                      0,
                      this.offset +
                          this.ctx.currentTime -
                          this.started -
                          (this.delay || 0),
                  ),
              )
            : this.offset;
    }
    async play() {
        if (!this.original) return;
        this.onPlaybackRequest?.();
        const request = ++this.generation;
        const context = this.context();
        await context.resume();
        if (request !== this.generation || !this.original) return;
        this.pause();
        const generation = ++this.generation;
        if (this.offset >= this.original.duration - 0.01) this.offset = 0;
        let worklet;
        if (this.master) {
            const prepared = await createLiveEqNode(context, this.original.numberOfChannels, this.analysis, this.settings, {
                onDelta: delta => { this.delta = delta; this.updateGains(); },
                onError: error => { this.onError?.(error); this.pause(); },
            });
            worklet = prepared.node;
            this.delay = prepared.delay;
            if (generation !== this.generation) {
                worklet.port.postMessage({ stop: true }); worklet.disconnect(); worklet.port.close(); return;
            }
        } else this.delay = 0;
        this.worklet = worklet;
        this.dryGain = context.createGain();
        this.wetGain = context.createGain();
        const delay = context.createDelay(1);
        delay.delayTime.value = this.delay;
        this.dry = context.createBufferSource();
        this.dry.buffer = this.original;
        this.dry.connect(delay);
        delay.connect(this.dryGain);
        this.dryGain.connect(this.mix);
        this.nodes = [this.dry, delay, this.dryGain, this.wetGain];
        if (this.master) {
            this.wet = context.createBufferSource();
            this.wet.buffer = this.master;
            this.wet.connect(worklet);
            worklet.connect(this.wetGain);
            this.wetGain.connect(this.mix);
            this.nodes.push(this.wet, worklet);
        }
        this.started = context.currentTime + 0.035;
        this.playing = true;
        this.updateGains(true);
        this.dry.start(this.started, this.offset);
        this.wet?.start(this.started, this.offset);
        this.ended = setTimeout(
            () => {
                if (generation === this.generation) {
                    this.pause();
                    this.offset = 0;
                    this.onState?.();
                }
            },
            (this.original.duration - this.offset + this.delay + 0.04) * 1000,
        );
        this.onState?.();
    }
    pause() {
        if (this.playing) this.offset = this.position();
        this.playing = false;
        this.generation++;
        clearTimeout(this.ended);
        for (const source of [this.dry, this.wet]) {
            try {
                source?.stop();
            } catch {}
        }
        for (const node of this.nodes || []) node.disconnect();
        this.worklet?.port.postMessage({ stop: true });
        this.worklet?.port.close();
        this.nodes = [];
        this.dry = null;
        this.wet = null;
        this.worklet = null;
        this.onState?.();
    }
    stop() {
        this.pause();
        this.offset = 0;
        this.onState?.();
    }
    async seek(seconds) {
        const resume = this.playing;
        this.pause();
        this.offset = Math.max(
            0,
            Math.min(this.original?.duration || 0, seconds),
        );
        if (resume) await this.play();
        this.onState?.();
    }
    select(source) {
        this.source = source;
        this.updateGains();
        this.onState?.();
    }
    updateSettings(settings, revision) {
        this.settings = structuredClone(settings);
        this.worklet?.port.postMessage({ settings: this.settings, revision });
    }
    updateGains(immediate = false) {
        if (!this.ctx) return;
        const reference = Math.min(
            this.before ?? -14,
            (this.after ?? -14) + (this.delta || 0),
        );
        const dry =
            this.source === "original"
                ? this.match && this.master
                    ? 10 ** ((reference - this.before) / 20)
                    : 1
                : 0;
        const wet =
            this.source === "master"
                ? this.match
                    ? 10 ** ((reference - this.after - (this.delta || 0)) / 20)
                    : 1
                : 0;
        for (const [node, value] of [
            [this.dryGain, dry],
            [this.wetGain, wet],
            [this.fader, this.volume],
        ])
            if (node) {
                if (immediate)
                    node.gain.setValueAtTime(value, this.ctx.currentTime);
                else
                    node.gain.setTargetAtTime(
                        value,
                        this.ctx.currentTime,
                        0.012,
                    );
            }
    }
}
