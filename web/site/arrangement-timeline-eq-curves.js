import { canvasSurface } from "./arrangement-timeline-waveform.js";

export function paintArrangementEq(canvas, current, proposed, sampleRate) {
    const surface = canvasSurface(canvas);
    if (!surface) return;
    const { ctx, width, height } = surface;
    const left = 32, right = width - 12, top = 12, bottom = height - 25;
    const upper = Math.min(20000, sampleRate * 0.49);
    const x = hz => left + Math.log(hz / 20) / Math.log(upper / 20) * (right - left);
    const y = db => top + (18 - db) / 36 * (bottom - top);
    ctx.font = "11px system-ui"; ctx.fillStyle = "#cbb7d8"; ctx.textAlign = "right";
    for (const db of [-18, -12, -6, 0, 6, 12, 18]) {
        ctx.strokeStyle = db === 0 ? "#79608a" : "#3a2b45";
        ctx.beginPath(); ctx.moveTo(left, y(db)); ctx.lineTo(right, y(db)); ctx.stroke();
        ctx.fillText(String(db), left - 5, y(db) + 4);
    }
    ctx.textAlign = "center";
    const label = hz => hz >= 1000 ? `${Number((hz / 1000).toFixed(2))}k` : String(hz);
    const finalLeft = right - ctx.measureText(label(upper)).width;
    let previousRight = -Infinity;
    for (const hz of [...[20, 100, 1000, 10000].filter(hz => hz < upper), upper]) {
        const text = label(hz), half = ctx.measureText(text).width / 2;
        const center = Math.min(right - half, x(hz));
        if (hz !== upper && (center - half < previousRight + 8 || center + half > finalLeft - 8)) continue;
        ctx.fillText(text, center, height - 7); previousRight = center + half;
    }
    ctx.save(); ctx.beginPath(); ctx.rect(left, top, right - left, bottom - top); ctx.clip();
    for (const [curve, color, dashed] of [[current, "#d7a0ff", false], [proposed, "#eadcc2", true]]) {
        if (!curve?.gain_db?.length || curve.frequencies?.length !== curve.gain_db.length) continue;
        ctx.strokeStyle = color; ctx.lineWidth = 2; ctx.setLineDash(dashed ? [5, 4] : []); ctx.beginPath();
        let first = true;
        for (let i = 0; i < curve.gain_db.length; i++) {
            const hz = curve.frequencies[i], db = curve.gain_db[i];
            if (!(hz > 0) || !Number.isFinite(db)) continue;
            first ? ctx.moveTo(x(hz), y(db)) : ctx.lineTo(x(hz), y(db)); first = false;
        }
        ctx.stroke();
    }
    ctx.restore();
}
