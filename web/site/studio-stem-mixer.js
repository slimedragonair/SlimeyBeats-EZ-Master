// Immutable AudioBuffers are shared across transport runs. Only AudioParams
// change while listening; no PCM is copied or rendered for a fader gesture.
export const STEM_SMOOTH_SECONDS = 0.012;
export const MAX_STEM_BYTES = 128 * 1024 * 1024;
const START_LEAD_SECONDS = 0.01;

export function stemPeaks(buffer) {
    return Array.from({ length: buffer.numberOfChannels }, (_, channel) => {
        let peak = 0;
        for (const sample of buffer.getChannelData(channel))
            peak = Math.max(peak, Math.abs(sample));
        return peak;
    });
}

function levels(bank, settings) {
    const solo = settings.tracks.some((track) => track.solo);
    const targets = settings.tracks.map((track) => {
        const stem = bank.get(track.kind);
        if (!stem) throw new Error(`Missing live stem: ${track.kind}`);
        const gain = track.mute || (solo && !track.solo)
            ? 0 : 10 ** ((track.gain_db + settings.output_gain_db) / 20);
        return stem.peaks.map((_, ch) => gain * Math.cos(
            Math.max(0, ch === 0 ? track.balance : -track.balance) * Math.PI / 2,
        ));
    });
    // A conservative sample headroom bound, not the offline true-peak guard.
    // Ramping all final gains together keeps the bound valid during edits too.
    const sum = new Array(bank.values().next().value.peaks.length).fill(0);
    settings.tracks.forEach((track, i) => bank.get(track.kind).peaks.forEach(
        (peak, ch) => { sum[ch] += peak * targets[i][ch]; },
    ));
    const peak = Math.max(...sum);
    const trim = settings.protect_peaks && peak > 0
        ? Math.min(1, 10 ** (settings.ceiling_dbtp / 20) / peak) : 1;
    return new Map(settings.tracks.map((track, i) => [
        track.kind, targets[i].map((gain) => gain * trim),
    ]));
}

const ramps = new WeakMap();
function smooth(param, value, now, immediate) {
    if (immediate) param.setValueAtTime(value, now);
    else {
        const previous = ramps.get(param);
        const fraction = previous ? Math.min(1, Math.max(0,
            (now - previous.time) / STEM_SMOOTH_SECONDS)) : 1;
        const held = previous
            ? previous.start + (previous.target - previous.start) * fraction : param.value;
        // Firefox versions without cancelAndHoldAtTime still retain the exact
        // value of our own linear ramp, including rapid overlapping gestures.
        if (param.cancelAndHoldAtTime) param.cancelAndHoldAtTime(now);
        else {
            param.cancelScheduledValues(now);
            param.setValueAtTime(held, now);
        }
        param.linearRampToValueAtTime(value, now + STEM_SMOOTH_SECONDS);
        ramps.set(param, { start: held, target: value, time: now });
    }
    if (immediate) ramps.set(param, { start: value, target: value, time: now });
}

export class StudioStemMixer {
    constructor(context, destination, bank, settings, offset, onended) {
        this.context = context;
        this.bank = bank;
        this.nodes = [];
        this.tracks = new Map();
        try {
            for (const [kind, stem] of bank) {
                const source = context.createBufferSource();
                source.buffer = stem.buffer;
                const splitter = context.createChannelSplitter(stem.peaks.length);
                const merger = context.createChannelMerger(stem.peaks.length);
                const gains = stem.peaks.map(() => context.createGain());
                this.nodes.push(source, splitter, merger, ...gains);
                source.connect(splitter);
                gains.forEach((gain, ch) => {
                    splitter.connect(gain, ch);
                    gain.connect(merger, 0, ch);
                });
                merger.connect(destination);
                this.tracks.set(kind, { source, gains });
            }
            this.update(settings, true);
            this.tracks.values().next().value.source.onended = onended;
            // Build the complete graph first, then schedule all stems against
            // one future clock instant, even for larger banks.
            this.started = context.currentTime + START_LEAD_SECONDS;
            for (const { source } of this.tracks.values()) source.start(this.started, offset);
        } catch (error) {
            this.stop();
            throw error;
        }
    }
    update(settings, immediate = false) {
        const targets = levels(this.bank, settings);
        const now = this.context.currentTime;
        for (const [kind, { gains }] of this.tracks)
            gains.forEach((gain, ch) => smooth(
                gain.gain, targets.get(kind)[ch], now, immediate,
            ));
    }
    stop() {
        for (const { source } of this.tracks.values()) {
            source.onended = null;
            try { source.stop(); } catch { /* May not have started after setup failure. */ }
        }
        for (const node of this.nodes) node.disconnect();
        this.nodes.length = 0;
    }
}
