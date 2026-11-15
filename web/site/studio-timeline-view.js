import { createEnvelopeCache } from "./studio-timeline-envelope.js";
import { rulerTicks } from "./studio-timeline-state.js";

function backing(canvas) {
    const width = canvas.clientWidth, height = canvas.clientHeight;
    const ratio = Math.min(2, globalThis.devicePixelRatio || 1);
    const w = Math.max(1, Math.round(width * ratio)), h = Math.max(1, Math.round(height * ratio));
    if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
    const ctx = canvas.getContext("2d");
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    return { ctx, width, height, key: `${w}:${h}` };
}

function playhead(element, model, frame) {
    element.hidden = frame < model.left || frame > model.right || !Number.isFinite(frame);
    if (!element.hidden) element.style.left = `${(frame - model.left) / model.span * 100}%`;
}

export function createTimelineView(host) {
    const viewport = host.querySelector("#s-time-lanes"), rows = host.querySelector(".s-time-rows");
    const ruler = host.querySelector(".s-time-ruler canvas"), cursor = host.querySelector(".s-time-ruler .s-time-playhead");
    const envelopes = createEnvelopeCache(), lanes = new Map();
    let rulerKey = "";
    function sync(entries, selected, disabled) {
        const kinds = new Set(entries.map((entry) => entry.kind));
        for (const [kind, lane] of lanes) if (!kinds.has(kind)) { lane.row.remove(); lanes.delete(kind); }
        entries.forEach((entry, index) => {
            let lane = lanes.get(entry.kind);
            if (!lane) {
                const row = document.createElement("div");
                row.className = "s-time-lane";
                row.dataset.kind = entry.kind;
                row.innerHTML = '<button type="button" class="s-time-lane-name"><span></span><small></small></button><div class="s-time-wave"><canvas width="1" height="1" aria-hidden="true"></canvas><span class="s-time-selection" hidden></span><span class="s-time-playhead" hidden></span></div>';
                lane = { row, button: row.querySelector("button"), wave: row.querySelector(".s-time-wave"),
                    canvas: row.querySelector("canvas"), selection: row.querySelector(".s-time-selection"),
                    cursor: row.querySelector(".s-time-playhead"), key: "" };
                lanes.set(entry.kind, lane);
            }
            if (lane.name !== entry.name) {
                lane.button.querySelector("span").textContent = entry.name;
                lane.button.title = `${entry.name} · ${entry.kind}`;
                lane.button.setAttribute("aria-label", `Select stem: ${entry.name} (${entry.kind})`);
                lane.name = entry.name;
            }
            if (lane.buffer !== entry.buffer) { lane.buffer = entry.buffer; lane.key = ""; }
            lane.button.disabled = disabled;
            lane.button.setAttribute("aria-pressed", String(entry.kind === selected));
            const selectedText = entry.kind === selected ? `Selected · ${entry.kind}` : entry.kind;
            if (lane.button.querySelector("small").textContent !== selectedText)
                lane.button.querySelector("small").textContent = selectedText;
            lane.row.dataset.selected = String(entry.kind === selected);
            if (rows.children[index] !== lane.row) rows.insertBefore(lane.row, rows.children[index] || null);
        });
    }
    function draw(model, seconds, selected) {
        if (!model.frames || viewport.clientWidth === 0) return;
        const frame = seconds * model.rate;
        const rulerSize = backing(ruler), key = `${rulerSize.key}:${model.left}:${model.right}:${model.rate}`;
        if (rulerKey !== key) {
            const { ctx, width, height } = rulerSize;
            ctx.clearRect(0, 0, width, height);
            ctx.fillStyle = "#bcb0c8";
            ctx.strokeStyle = "#55405f";
            ctx.font = "12px system-ui";
            let lastRight = -4;
            for (const tick of rulerTicks(model.left, model.right, model.rate, width)) {
                ctx.beginPath(); ctx.moveTo(tick.x, height - 6); ctx.lineTo(tick.x, height); ctx.stroke();
                const textWidth = ctx.measureText(tick.label).width;
                const x = Math.max(4, Math.min(width - textWidth - 4, tick.x + 4));
                if (x >= lastRight + 4) { ctx.fillText(tick.label, x, 17); lastRight = x + textWidth; }
            }
            rulerKey = key;
        }
        playhead(cursor, model, frame);
        const top = viewport.scrollTop, bottom = top + viewport.clientHeight;
        for (const [kind, lane] of lanes) {
            if (lane.row.offsetTop + lane.row.offsetHeight < top || lane.row.offsetTop > bottom) {
                // Offscreen lanes retain their envelope, but release large pixel buffers.
                if (lane.canvas.width !== 1) { lane.canvas.width = lane.canvas.height = 1; lane.key = ""; }
                continue;
            }
            const { ctx, width, height, key: sizeKey } = backing(lane.canvas);
            const waveKey = `${sizeKey}:${model.left}:${model.right}:${model.rate}`;
            if (lane.key !== waveKey) {
                const envelope = envelopes.get(lane.buffer), center = height / 2, scale = center - 5;
                ctx.clearRect(0, 0, width, height);
                ctx.strokeStyle = "#55405f";
                ctx.beginPath(); ctx.moveTo(0, center); ctx.lineTo(width, center); ctx.stroke();
                ctx.strokeStyle = "#78dbd0";
                ctx.beginPath();
                for (let x = 0; x < width; x++) {
                    const first = (model.left + x / width * model.span) / model.rate * lane.buffer.sampleRate;
                    const last = (model.left + (x + 1) / width * model.span) / model.rate * lane.buffer.sampleRate;
                    const [low, high] = envelope.range(first, last);
                    ctx.moveTo(x + 0.5, center - Math.min(1, high) * scale);
                    ctx.lineTo(x + 0.5, center - Math.max(-1, low) * scale);
                }
                ctx.stroke();
                lane.key = waveKey;
            }
            lane.selection.hidden = kind !== selected || !model.hasSelection || model.end <= model.left || model.start >= model.right;
            if (!lane.selection.hidden) {
                const left = Math.max(0, (model.start - model.left) / model.span * 100);
                const right = Math.min(100, (model.end - model.left) / model.span * 100);
                lane.selection.style.left = `${left}%`;
                lane.selection.style.width = `${right - left}%`;
            }
            playhead(lane.cursor, model, frame);
        }
    }
    function clear() {
        rows.replaceChildren(); lanes.clear(); envelopes.reset(); rulerKey = "";
        ruler.width = ruler.height = 1; cursor.hidden = true; viewport.scrollTop = 0;
    }
    function reveal(kind) {
        const row = lanes.get(kind)?.row;
        if (!row) return;
        if (row.offsetTop < viewport.scrollTop + 30) viewport.scrollTop = Math.max(0, row.offsetTop - 30);
        else if (row.offsetTop + row.offsetHeight > viewport.scrollTop + viewport.clientHeight)
            viewport.scrollTop = row.offsetTop + row.offsetHeight - viewport.clientHeight;
    }
    return { sync, draw, clear, reveal, viewport };
}
