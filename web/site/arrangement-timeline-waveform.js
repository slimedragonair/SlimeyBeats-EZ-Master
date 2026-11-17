export function canvasSurface(canvas) {
    const width = canvas.clientWidth, height = canvas.clientHeight;
    if (!width || !height) return null;
    const ratio = Math.min(2, globalThis.devicePixelRatio || 1);
    const w = Math.max(1, Math.round(width * ratio)), h = Math.max(1, Math.round(height * ratio));
    if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
    const ctx = canvas.getContext("2d");
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, width, height);
    return { ctx, width, height };
}

// Envelopes are bounded numeric metadata from the immutable asset. Drawing does
// not read PCM, apply gain/EQ, or invent detail while a waveform is unavailable.
export function paintArrangementEnvelope(canvas, envelope, selected) {
    const surface = canvasSurface(canvas);
    if (!surface) return;
    const { ctx, width, height } = surface;
    ctx.strokeStyle = "#e0d2e744";
    ctx.beginPath(); ctx.moveTo(0, height / 2); ctx.lineTo(width, height / 2); ctx.stroke();
    if (!envelope?.peaks?.length) {
        ctx.fillStyle = "#e0d2e7"; ctx.font = "11px system-ui";
        if (width > 110) ctx.fillText(envelope?.error ? "Waveform unavailable" : envelope?.deferred ? "Zoom for waveform detail" : "Loading waveform…", 16, height / 2 - 6);
        return;
    }
    const peaks = envelope.peaks;
    const amplitude = height * 0.46;
    ctx.strokeStyle = selected ? "#b5fff0" : "#e0c1fb";
    ctx.lineWidth = Math.max(1, width / peaks.length * 0.8);
    ctx.beginPath();
    for (let i = 0; i < peaks.length; i++) {
        const [low, high] = peaks[i];
        if (!Number.isFinite(low) || !Number.isFinite(high)) continue;
        const x = (i + 0.5) / peaks.length * width;
        ctx.moveTo(x, height / 2 - Math.min(1, high) * amplitude);
        ctx.lineTo(x, height / 2 - Math.max(-1, low) * amplitude);
    }
    ctx.stroke();
}
