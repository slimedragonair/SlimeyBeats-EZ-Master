import { rulerTicks } from "./studio-timeline-state.js";
import { canvasSurface, paintArrangementEnvelope } from "./arrangement-timeline-waveform.js";
import { trackLabel } from "./arrangement-timeline-state.js";

function button(text, label) {
    const element = document.createElement("button");
    element.type = "button"; element.textContent = text;
    if (label) element.setAttribute("aria-label", label);
    return element;
}

export function createArrangementTimelineView(host, { getEnvelope, selectLane, selectClip, toggleLane }) {
    const viewport = host.querySelector('[data-arr="lanes"]'), rows = host.querySelector('[data-arr="rows"]');
    const ruler = host.querySelector('[data-arr="ruler"]'), mounted = new Map();
    let model, assets = new Map(), busy = false, position = 0, disposed = false;
    const laneHeight = () => Number.parseFloat(getComputedStyle(viewport).getPropertyValue("--arr-lane-height")) || 132;
    function newLane(lane) {
        const row = document.createElement("div"); row.className = "arr-lane"; row.dataset.laneId = lane.id;
        const head = document.createElement("div"); head.className = "arr-lane-head";
        const handle = button("↕", `Reorder track ${lane.name}`); handle.className = "arr-lane-handle"; handle.dataset.laneHandle = lane.id;
        const name = button(lane.name, `Select track ${lane.name}`); name.className = "arr-lane-name";
        name.onclick = () => selectLane(lane.id);
        const controls = document.createElement("div"); controls.className = "arr-lane-buttons";
        const mute = button("Mute"), solo = button("Solo");
        mute.onclick = () => toggleLane(lane.id, "mute"); solo.onclick = () => toggleLane(lane.id, "solo");
        controls.append(mute, solo); head.append(handle, name, controls);
        const wave = document.createElement("div"); wave.className = "arr-lane-wave";
        const range = document.createElement("div"); range.className = "arr-range";
        const cursor = document.createElement("div"); cursor.className = "arr-playhead";
        wave.append(range, cursor); row.append(head, wave); rows.append(row);
        return { row, head, handle, name, mute, solo, wave, range, cursor, clips: new Map() };
    }
    function newClip(clip, lane, entry) {
        const element = document.createElement("div"); element.className = "arr-clip"; element.dataset.clipId = clip.id;
        const body = button(""); body.className = "arr-clip-body";
        const label = document.createElement("span"); label.className = "arr-clip-label";
        const canvas = document.createElement("canvas"); canvas.setAttribute("aria-hidden", "true");
        body.append(label, canvas); body.onclick = () => selectClip(clip.id);
        const first = button("│", "Trim clip start"), last = button("│", "Trim clip end");
        first.className = "arr-trim arr-trim-start"; first.dataset.trim = "start";
        last.className = "arr-trim arr-trim-end"; last.dataset.trim = "end";
        element.append(body, first, last); entry.wave.append(element);
        return { element, body, label, canvas, first, last };
    }
    function drawRuler() {
        const surface = canvasSurface(ruler);
        if (!surface || !model) return;
        const { ctx, width, height } = surface;
        ctx.font = "11px system-ui"; ctx.fillStyle = "#d2bfdc"; ctx.strokeStyle = "#8d68a6"; ctx.textAlign = "left";
        const firstLabel = `${Number((model.left / model.rate).toFixed(3))} s`;
        const lastLabel = `${Number((model.right / model.rate).toFixed(3))} s`;
        const firstWidth = ctx.measureText(firstLabel).width, lastWidth = ctx.measureText(lastLabel).width;
        ctx.fillText(firstLabel, 2, 16); ctx.fillText(lastLabel, width - lastWidth - 2, 16);
        for (const tick of rulerTicks(model.left, model.right, model.rate, width)) {
            if (tick.x > firstWidth + 12 && tick.x + ctx.measureText(tick.label).width < width - lastWidth - 12)
                ctx.fillText(tick.label, tick.x + 2, 16);
            ctx.beginPath(); ctx.moveTo(tick.x, height - 8); ctx.lineTo(tick.x, height); ctx.stroke();
        }
    }
    function paintCursor() {
        if (!model) return;
        const percent = (position - model.left) / model.span * 100;
        for (const entry of mounted.values()) {
            entry.cursor.hidden = percent < 0 || percent > 100;
            entry.cursor.style.left = `${percent}%`;
        }
    }
    function render() {
        if (!model || disposed || !viewport.clientWidth) return;
        const height = laneHeight(), top = Math.max(0, viewport.scrollTop - ruler.clientHeight);
        const first = Math.max(0, Math.floor(top / height) - 2);
        const last = Math.min(model.lanes.length, Math.ceil((top + viewport.clientHeight) / height) + 2);
        rows.style.height = `${model.lanes.length * height}px`;
        const visible = new Set(model.lanes.slice(first, last).map(lane => lane.id));
        for (const [id, entry] of mounted) if (!visible.has(id)) { entry.row.remove(); mounted.delete(id); }
        for (let index = first; index < last; index++) {
            const lane = model.lanes[index];
            let entry = mounted.get(lane.id);
            if (!entry) { entry = newLane(lane); mounted.set(lane.id, entry); }
            entry.row.style.top = `${index * height}px`;
            entry.row.dataset.selected = String(lane.id === model.selectedLane);
            entry.name.textContent = trackLabel(lane, index); entry.name.title = lane.name;
            entry.name.setAttribute("aria-label", `Select track ${index + 1}, ${lane.name}`);
            for (const [control, kind] of [[entry.mute, "mute"], [entry.solo, "solo"]]) {
                control.disabled = busy; control.setAttribute("aria-pressed", String(lane[kind]));
                control.setAttribute("aria-label", `${kind === "mute" ? "Mute" : "Solo"} ${lane.name}`);
            }
            entry.handle.disabled = busy;
            const clips = lane.clips.filter(clip => !(model.gesture?.clip.id === clip.id && model.gesture.lane_id !== lane.id));
            if (model.gesture?.lane_id === lane.id && !clips.some(clip => clip.id === model.gesture.clip.id)) clips.push(model.gesture.clip);
            const shown = new Set();
            for (const original of clips) {
                const clip = model.gesture?.clip.id === original.id ? model.gesture.clip : original;
                if (clip.end_frame <= model.left || clip.start_frame >= model.right) continue;
                shown.add(clip.id);
                let item = entry.clips.get(clip.id);
                if (!item) { item = newClip(clip, lane, entry); entry.clips.set(clip.id, item); }
                const firstFrame = Math.max(clip.start_frame, model.left), lastFrame = Math.min(clip.end_frame, model.right);
                item.element.style.left = `${(firstFrame - model.left) / model.span * 100}%`;
                item.element.style.width = `${(lastFrame - firstFrame) / model.span * 100}%`;
                item.element.dataset.selected = String(clip.id === model.selectedClip);
                item.element.dataset.gesture = String(clip.id === model.gesture?.clip.id);
                const name = clip.name || assets.get(clip.source_id)?.name || "Audio clip";
                item.label.textContent = name;
                item.body.title = `${name} · ${clip.id} · ${(clip.start_frame / model.rate).toFixed(3)}–${(clip.end_frame / model.rate).toFixed(3)} s`;
                item.body.setAttribute("aria-label", `Select clip ${name} in ${lane.name}, ${clip.start_frame / model.rate} to ${clip.end_frame / model.rate} seconds`);
                item.body.disabled = busy;
                item.first.hidden = firstFrame !== clip.start_frame || item.element.clientWidth < 44;
                item.last.hidden = lastFrame !== clip.end_frame || item.element.clientWidth < 44;
                item.first.disabled = item.last.disabled = busy;
                const sourceFirst = clip.source_start_frame + firstFrame - clip.start_frame;
                const envelope = getEnvelope(clip.source_id, sourceFirst, sourceFirst + lastFrame - firstFrame, Math.max(1, Math.ceil(item.element.clientWidth)));
                paintArrangementEnvelope(item.canvas, envelope, clip.id === model.selectedClip);
            }
            for (const [id, clip] of entry.clips) if (!shown.has(id)) { clip.element.remove(); entry.clips.delete(id); }
            const selected = lane.id === model.selectedLane && model.hasRange;
            entry.range.hidden = !selected || model.end <= model.left || model.start >= model.right;
            entry.range.style.left = `${Math.max(0, (model.start - model.left) / model.span * 100)}%`;
            entry.range.style.width = `${(Math.min(model.right, model.end) - Math.max(model.left, model.start)) / model.span * 100}%`;
        }
        drawRuler(); paintCursor();
    }
    const resize = new ResizeObserver(render); resize.observe(viewport);
    viewport.addEventListener("scroll", render, { passive: true });
    return {
        viewport,
        sync(next, options = {}) { model = next; assets = new Map((options.assets || []).map(asset => [asset.id, asset])); busy = !!options.busy; render(); },
        render,
        position(frame) { if (position !== frame) { position = frame; paintCursor(); } },
        laneAt(clientY) {
            const rect = rows.getBoundingClientRect();
            return model?.lanes[Math.max(0, Math.min(model.lanes.length - 1, Math.floor((clientY - rect.top) / laneHeight())))];
        },
        waveRect(laneId) { return mounted.get(laneId)?.wave.getBoundingClientRect(); },
        reveal(laneId) { const index = model?.lanes.findIndex(lane => lane.id === laneId); if (index >= 0) { viewport.scrollTop = index * laneHeight(); render(); } },
        metrics() { return { lanes: mounted.size, clips: [...mounted.values()].reduce((count, lane) => count + lane.clips.size, 0) }; },
        dispose() { disposed = true; resize.disconnect(); viewport.removeEventListener("scroll", render); rows.replaceChildren(); mounted.clear(); },
    };
}
