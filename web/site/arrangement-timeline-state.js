const EMPTY_VIEW_SECONDS = 10;
const MIN_VIEW_SECONDS = 0.01;
const MIN_EQ_RANGE_SECONDS = 0.02;
const clamp = (value, low, high) => Math.max(low, Math.min(high, value));

export function frameNumber(value, name = "Frame") {
    if (!Number.isSafeInteger(value) || value < 0) throw new Error(`${name} must be a nonnegative, exact frame number.`);
    return value;
}

export function secondsToFrame(text, rate) {
    if (typeof text !== "string" || !text.trim()) throw new Error("Enter a time in seconds.");
    const seconds = Number(text);
    if (!Number.isFinite(seconds) || seconds < 0 || !Number.isSafeInteger(rate) || rate <= 0)
        throw new Error("Enter a finite time at or after zero.");
    return frameNumber(Math.round(seconds * rate));
}

export function frameSeconds(frame, rate) { return rate ? String(frameNumber(frame) / rate) : "0"; }
export function clipDuration(clip) { return frameNumber(clip.end_frame - clip.start_frame, "Clip duration"); }
export function trackLabel(lane, index) { return `${index + 1} · ${lane.name}`; }

// Only selection, navigation and uncommitted gesture geometry live here. Audio
// and history remain in the shared arrangement engine behind the adapter.
export class ArrangementTimelineState {
    constructor() { this.reset(); }
    reset() {
        this.arrangement = null;
        this.revision = null;
        this.selectedLane = this.selectedClip = null;
        this.start = this.end = 0;
        this.left = 0;
        this.right = EMPTY_VIEW_SECONDS * 48000;
        this.mode = "move";
        this.scope = "mix";
        this.rangeTarget = "lane";
        this.gesture = null;
        this.sourceNames = new Map();
    }
    get rate() { return this.arrangement?.sample_rate || 48000; }
    get lanes() { return this.arrangement?.lanes || []; }
    get frames() {
        return this.lanes.reduce((end, lane) => lane.clips.reduce((last, clip) => Math.max(last, clip.end_frame), end), 0);
    }
    get extent() { return Math.max(this.frames, this.rate * EMPTY_VIEW_SECONDS); }
    get span() { return this.right - this.left; }
    get minimumSpan() { return Math.max(1, Math.round(this.rate * MIN_VIEW_SECONDS)); }
    get hasRange() { return this.end > this.start; }
    lane(id = this.selectedLane) { return this.lanes.find(lane => lane.id === id); }
    clip(id = this.selectedClip) {
        for (const lane of this.lanes) {
            const clip = lane.clips.find(clip => clip.id === id);
            if (clip) return { lane, clip };
        }
        return null;
    }
    configure(arrangement, revision, assets = []) {
        const previousRate = this.rate;
        const firstImport = this.frames === 0 && this.sourceNames.size === 0;
        const fresh = !this.arrangement || previousRate !== arrangement.sample_rate;
        if (!Number.isSafeInteger(arrangement.sample_rate) || arrangement.sample_rate <= 0 || !Array.isArray(arrangement.lanes))
            throw new Error("The arrangement metadata has no valid rate or lanes.");
        for (const lane of arrangement.lanes) for (const clip of lane.clips) {
            frameNumber(clip.start_frame); frameNumber(clip.end_frame); frameNumber(clip.source_start_frame);
            if (!clipDuration(clip)) throw new Error("The arrangement contains an empty clip.");
        }
        this.arrangement = arrangement;
        this.revision = revision;
        this.sourceNames = new Map(assets.map(asset => [asset.id, asset.name]));
        if (!this.clip()) this.selectedClip = null;
        if (!this.lane()) this.selectedLane = null;
        if (this.selectedClip) this.selectedLane = this.clip().lane.id;
        this.start = Math.min(this.start, this.extent);
        this.end = clamp(this.end, this.start, this.extent);
        this.gesture = null;
        if (fresh || firstImport && this.frames > 0) this.fit();
        else this.setView(this.left, this.span);
    }
    selectLane(id) {
        if (!id) { this.selectedLane = this.selectedClip = null; return true; }
        if (!this.lane(id)) return false;
        this.selectedLane = id;
        if (this.clip()?.lane.id !== id) this.selectedClip = null;
        return true;
    }
    selectClip(id) {
        if (!id) { this.selectedClip = null; return true; }
        const entry = this.clip(id);
        if (!entry) return false;
        this.selectedLane = entry.lane.id;
        this.selectedClip = id;
        return true;
    }
    selectRange(first, last) {
        const limit = Math.max(1, this.frames);
        first = clamp(Math.round(first), 0, limit - 1);
        last = clamp(Math.round(last), 0, limit);
        this.start = Math.min(first, last);
        this.end = Math.max(this.start + 1, Math.max(first, last));
    }
    setRange(first, last) {
        frameNumber(first); frameNumber(last);
        if (last <= first || last > this.frames) throw new Error("Choose a nonempty range within the arrangement duration.");
        this.start = first; this.end = last;
    }
    setView(left, span) {
        const width = clamp(Math.round(span), this.minimumSpan, this.extent);
        this.left = clamp(Math.round(left), 0, this.extent - width);
        this.right = this.left + width;
    }
    fit() { this.left = 0; this.right = Math.max(this.frames, this.rate); }
    fitSelection() {
        const clip = this.clip()?.clip;
        const first = this.hasRange ? this.start : clip?.start_frame;
        const last = this.hasRange ? this.end : clip?.end_frame;
        if (first == null || last == null) return false;
        const span = Math.max(this.minimumSpan, last - first);
        this.setView((first + last - span) / 2, span);
        return true;
    }
    zoom(factor, focus = (this.left + this.right) / 2) {
        const fraction = this.span ? clamp((focus - this.left) / this.span, 0, 1) : 0.5;
        const span = clamp(Math.round(this.span * factor), this.minimumSpan, this.extent);
        this.setView(focus - fraction * span, span);
    }
    pan(direction) { this.setView(this.left + Math.round(this.span * 0.75) * direction, this.span); }
    frameAt(clientX, rect) {
        return clamp(Math.round(this.left + (clientX - rect.left) / Math.max(1, rect.width) * this.span), 0, this.extent);
    }
    movePreview(id, start, laneId) {
        const entry = this.clip(id);
        if (!entry || !this.lane(laneId)) throw new Error("Choose an available clip and track.");
        const first = frameNumber(Math.max(0, Math.round(start)));
        return { lane_id: laneId, clip: { ...entry.clip, start_frame: first,
            end_frame: frameNumber(first + clipDuration(entry.clip)) } };
    }
    trimPreview(id, edge, frame, sourceFrames) {
        const entry = this.clip(id);
        if (!entry) throw new Error("Choose an available clip.");
        const clip = { ...entry.clip };
        if (edge === "start") {
            const first = clamp(Math.round(frame), Math.max(0, clip.start_frame - clip.source_start_frame), clip.end_frame - 1);
            clip.source_start_frame += first - clip.start_frame;
            clip.start_frame = first;
        } else if (edge === "end") {
            frameNumber(sourceFrames, "Source length");
            clip.end_frame = clamp(Math.round(frame), clip.start_frame + 1,
                clip.start_frame + sourceFrames - clip.source_start_frame);
        } else throw new Error("Choose the start or end trim handle.");
        return { lane_id: entry.lane.id, clip };
    }
    eqTarget() {
        const lane = this.lane(), entry = this.clip();
        let scope, label;
        const clipRange = this.scope === "range" && this.rangeTarget === "clip";
        if (this.scope === "clip" || clipRange) {
            if (!entry) return { error: "Select a clip for clip EQ." };
            scope = { kind: "clip", clip_id: entry.clip.id };
            const index = entry.lane.clips.findIndex(clip => clip.id === entry.clip.id) + 1;
            const name = entry.clip.name || this.sourceNames.get(entry.clip.source_id) || "Audio clip";
            label = `Clip ${index} · ${name} · track ${trackLabel(entry.lane, this.lanes.indexOf(entry.lane))} · song ${frameSeconds(entry.clip.start_frame, this.rate)}–${frameSeconds(entry.clip.end_frame, this.rate)} s`;
        } else if (this.scope === "lane" || (this.scope === "range" && this.rangeTarget === "lane")) {
            if (!lane) return { error: "Select a track for track EQ." };
            scope = { kind: "lane", lane_id: lane.id }; label = `Track ${trackLabel(lane, this.lanes.indexOf(lane))}`;
        } else { scope = { kind: "mix" }; label = "Whole mix"; }
        if (this.scope === "range") {
            if (!this.hasRange) return { error: "Select or type a nonempty time range for EQ." };
            if (this.end - this.start < Math.ceil(this.rate * MIN_EQ_RANGE_SECONDS))
                return { error: "A time-limited EQ effect needs at least 20 ms. Clip editing still supports single frames." };
            if (clipRange) {
                const clip = entry.clip;
                if (this.start < clip.start_frame || this.end > clip.end_frame)
                    return { error: "Keep a clip EQ time range inside the selected clip." };
                const first = clip.source_start_frame + this.start - clip.start_frame;
                const last = clip.source_start_frame + this.end - clip.start_frame;
                return { scope, range: { start_frame: first, end_frame: last },
                    label: `${label} · song ${frameSeconds(this.start, this.rate)}–${frameSeconds(this.end, this.rate)} s; source ${frameSeconds(first, this.rate)}–${frameSeconds(last, this.rate)} s (end exclusive)` };
            }
            return { scope, range: { start_frame: this.start, end_frame: this.end },
                label: `${label} · ${frameSeconds(this.start, this.rate)}–${frameSeconds(this.end, this.rate)} s (end exclusive)` };
        }
        if (this.scope === "clip") {
            const clip = entry.clip;
            return { scope, range: null, label: `${label}; source ${frameSeconds(clip.source_start_frame, this.rate)}–${frameSeconds(clip.source_start_frame + clipDuration(clip), this.rate)} s · entire clip` };
        }
        return { scope, range: null, label: `${label} · entire ${this.scope === "mix" ? "arrangement" : "track"} · 0–${frameSeconds(this.frames, this.rate)} s` };
    }
}
