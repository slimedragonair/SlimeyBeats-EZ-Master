// All selections and view bounds use integer sample frames; the end is exclusive.
export const MIN_VIEW_SECONDS = 0.01;
export const MIN_GAIN_DB = -60;
export const MAX_GAIN_DB = 12;
const clamp = (value, low, high) => Math.max(low, Math.min(high, value));

export class TimelineState {
    constructor() { this.reset(); }
    reset() {
        this.rate = this.frames = this.start = this.end = 0;
        this.left = this.right = 0;
    }
    configure(buffer) {
        if (!buffer) { this.reset(); return; }
        const fresh = !this.frames || this.rate !== buffer.sampleRate;
        this.rate = buffer.sampleRate;
        this.frames = buffer.length;
        this.start = clamp(this.start, 0, this.frames);
        this.end = clamp(this.end, this.start, this.frames);
        if (fresh) this.fit();
        else this.setView(this.left, this.right - this.left);
    }
    get minimumSpan() {
        return Math.min(this.frames, Math.max(1, Math.round(this.rate * MIN_VIEW_SECONDS)));
    }
    get span() { return this.right - this.left; }
    get hasSelection() { return this.end > this.start; }
    setSeconds(start, end) {
        const error = rangeError(start, end, this.rate, this.frames);
        if (error) return error;
        this.start = clamp(Math.round(start * this.rate), 0, this.frames);
        this.end = clamp(Math.round(end * this.rate), this.start, this.frames);
        return "";
    }
    selectFrames(first, last) {
        this.start = clamp(Math.round(Math.min(first, last)), 0, this.frames - 1);
        this.end = clamp(Math.round(Math.max(first, last)), this.start + 1, this.frames);
    }
    setView(left, span) {
        const width = clamp(Math.round(span), this.minimumSpan, this.frames);
        this.left = clamp(Math.round(left), 0, this.frames - width);
        this.right = this.left + width;
    }
    fit() { this.left = 0; this.right = this.frames; }
    fitSelection() {
        if (!this.hasSelection) return;
        const span = Math.max(this.minimumSpan, this.end - this.start);
        this.setView((this.start + this.end - span) / 2, span);
    }
    zoom(factor, anchor = (this.left + this.right) / 2) {
        const span = clamp(Math.round(this.span * factor), this.minimumSpan, this.frames);
        const fraction = this.span ? clamp((anchor - this.left) / this.span, 0, 1) : 0.5;
        this.setView(anchor - span * fraction, span);
    }
    pan(direction) { this.setView(this.left + direction * Math.max(1, Math.round(this.span * 0.75)), this.span); }
    edit(operation) {
        return { start_seconds: this.start / this.rate, end_seconds: this.end / this.rate, operation };
    }
}

export function rangeError(start, end, rate, frames) {
    if (!Number.isFinite(start) || !Number.isFinite(end))
        return "Enter both selection times in seconds.";
    if (!rate || start < 0 || end < 0 || start > frames / rate + 1e-9 || end > frames / rate + 1e-9)
        return "Keep both times within the stem duration.";
    return end < start ? "End must be at or after start." : "";
}

export function gainError(value) {
    return Number.isFinite(value) && value >= MIN_GAIN_DB && value <= MAX_GAIN_DB
        ? "" : "Enter a gain from −60 to +12 dB.";
}

export function frameText(frame, rate) { return rate ? String(frame / rate) : "0"; }

export function rulerTicks(left, right, rate, width) {
    if (!rate || right <= left) return [];
    const seconds = (right - left) / rate;
    const wanted = seconds / Math.max(1, Math.floor(width / 92));
    const base = 10 ** Math.floor(Math.log10(wanted));
    const step = [1, 2, 5, 10].map((value) => value * base).find((value) => value >= wanted);
    const ticks = [];
    for (let i = Math.ceil(left / rate / step); i * step <= right / rate + step * 1e-6; i++) {
        const time = i * step;
        ticks.push({ x: (time * rate - left) / (right - left) * width,
            label: `${time.toFixed(Math.max(0, -Math.floor(Math.log10(step))))} s` });
    }
    return ticks;
}
