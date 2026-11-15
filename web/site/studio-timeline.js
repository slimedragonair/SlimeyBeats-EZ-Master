import { kindLabel } from "./studio-stems.js";
import { timelineTemplate } from "./studio-timeline-template.js";
import { TimelineState, frameText, rangeError, gainError } from "./studio-timeline-state.js";
import { createTimelineView } from "./studio-timeline-view.js";
import { wireTimelineInteraction } from "./studio-timeline-interaction.js";

// Parent owns jobs, transport, undo and recovery. This component only submits
// {kind, edit}; sync is safe at playback cadence and draw never reads new PCM
// unless an immutable buffer first becomes visible (or reset cleared the cache).
export function createStudioTimeline(host, { getState, getStems, position, seek, apply, notice }) {
    host.innerHTML = timelineTemplate;
    const $ = (id) => host.querySelector(`#${id}`), model = new TimelineState();
    const view = createTimelineView(host), stem = $("s-editor-stem");
    const start = $("s-edit-start"), end = $("s-edit-end"), gain = $("s-edit-gain");
    const tools = { "s-edit-silence": "silence", "s-edit-fade-in": "fade_in",
        "s-edit-fade-out": "fade_out", "s-edit-reverse": "reverse", "s-edit-apply-gain": "gain" };
    let entries = [], kind, optionKey = "", draft = false, submitting = false, generation = 0;
    const value = (input) => input.value.trim() === "" ? NaN : Number(input.value);
    const text = (id, content) => { if ($(id).textContent !== content) $(id).textContent = content; };
    function blocked(edit = false) {
        const state = getState();
        if (state.active === false) return "Open Studio to edit a stem.";
        if (state.busy || submitting) return "Wait for the current Studio job to finish.";
        if (!state.info?.loaded || !model.frames || !getStems()?.has(kind)) return "Import aligned stems to enable time edits.";
        if (edit && state.pendingMix) return "Update mix preview before applying time edits.";
        return "";
    }
    function timeError() { return rangeError(value(start), value(end), model.rate, model.frames); }
    function paintFields(force = false) {
        if (draft && !force) return;
        for (const [input, frame] of [[start, model.start], [end, model.end]]) {
            if (force || document.activeElement !== input) input.value = frameText(frame, model.rate);
            input.max = frameText(model.frames, model.rate);
            input.step = model.rate ? String(1 / model.rate) : "any";
        }
    }
    function disable(id, reason) {
        const control = $(id);
        control.disabled = Boolean(reason);
        control.setAttribute("aria-describedby", "s-time-reason");
        if (id === "s-edit-apply-gain") control.setAttribute("aria-describedby", "s-time-reason s-time-gain-error");
        if (reason) { control.dataset.disabledReason = reason; control.title = reason; }
        else if (control.dataset.disabledReason) {
            control.title = control.dataset.toolHelp || "";
            delete control.dataset.disabledReason;
        }
    }
    function controls() {
        const navigation = blocked(), editing = blocked(true), invalid = timeError();
        const empty = !model.hasSelection || (draft && Math.round(value(start) * model.rate) >= Math.round(value(end) * model.rate));
        const reason = editing || invalid || (empty ? "Select at least one frame; end is exclusive." : "");
        const badGain = gainError(value(gain));
        for (const id of Object.keys(tools)) disable(id, reason || (tools[id] === "gain" ? badGain : ""));
        for (const input of [stem, start, end, gain]) input.disabled = Boolean(editing);
        for (const input of [start, end]) input.setAttribute("aria-invalid", String(Boolean(invalid)));
        gain.setAttribute("aria-invalid", String(Boolean(badGain)));
        text("s-time-reason", reason || "Ready. A tool applies only to this stem and selection.");
        text("s-time-gain-error", badGain);
        disable("s-time-fit", navigation);
        disable("s-time-fit-selection", navigation || invalid || (empty ? "Select a range to fit it in view." : ""));
        disable("s-time-zoom-in", navigation || (model.span <= model.minimumSpan ? "Minimum view reached: 10 ms, or the full shorter stem." : ""));
        disable("s-time-zoom-out", navigation || (model.span >= model.frames ? "The full song is already visible." : ""));
        disable("s-time-left", navigation || (model.left <= 0 ? "The view is at the start." : ""));
        disable("s-time-right", navigation || (model.right >= model.frames ? "The view is at the end." : ""));
        disable("s-time-seek-selection", navigation || invalid);
        const seconds = (frame) => model.rate ? (frame / model.rate).toFixed(6).replace(/0+$/, "").replace(/\.$/, "") : "0";
        text("s-time-view", `View ${seconds(model.left)}–${seconds(model.right)} s · minimum view 10 ms`);
        text("s-time-selection", draft ? (invalid || "Finish typing; Enter or leaving the field commits frame rounding.")
            : `Frames ${model.start}–${model.end} (end exclusive) · ${model.end - model.start} frame${model.end - model.start === 1 ? "" : "s"} selected`);
        host.setAttribute("aria-busy", String(Boolean(getState().busy || submitting)));
        view.sync(entries, kind, Boolean(editing));
    }
    function commitTimes() {
        const error = model.setSeconds(value(start), value(end));
        draft = Boolean(error);
        if (!error) paintFields(true);
        controls(); draw();
        return !error;
    }
    function choose(next) {
        if (blocked(true) || !entries.some((entry) => entry.kind === next)) return;
        kind = next; stem.value = kind;
        model.configure(entries.find((entry) => entry.kind === kind).buffer);
        sync(); draw();
    }
    function repaint(selection = false) {
        if (selection) { draft = false; paintFields(true); }
        controls(); draw();
    }
    const interaction = wireTimelineInteraction(view.viewport, {
        model: () => model, allowed: () => !blocked(true), choose, selected: () => kind,
        seek: (seconds) => seek(seconds), repaint,
    });
    function sync({ reset: clear = false } = {}) {
        if (clear) reset();
        const state = getState(), bank = getStems();
        entries = state.info?.loaded ? [...(bank || new Map())].filter(([, entry]) => entry?.buffer?.length > 0)
            .map(([kind, entry]) => ({ kind, buffer: entry.buffer, name: String(state.info.stem_names?.[kind] || kindLabel(kind)) })) : [];
        host.hidden = entries.length === 0;
        if (!entries.length) {
            if (model.frames) { interaction.cancel(); model.reset(); view.clear(); draft = false; }
            kind = undefined;
        } else {
            if (!entries.some((entry) => entry.kind === kind)) { kind = entries[0].kind; draft = false; }
            model.configure(entries.find((entry) => entry.kind === kind).buffer);
        }
        const nextKey = JSON.stringify(entries.map((entry) => [entry.kind, entry.name]));
        if (nextKey !== optionKey) {
            stem.replaceChildren(...entries.map((entry) => {
                const option = document.createElement("option");
                option.value = entry.kind; option.textContent = `${entry.name} · ${entry.kind}`; return option;
            }));
            optionKey = nextKey;
        }
        stem.value = kind || "";
        paintFields(); controls(); draw();
    }
    function draw() {
        if (host.hidden || getState().active === false) return;
        view.draw(model, position(), kind);
    }
    function reset() {
        interaction.cancel(); generation++; submitting = false; draft = false;
        entries = []; kind = undefined; optionKey = "";
        model.reset(); view.clear(); stem.replaceChildren(); gain.value = "0";
        paintFields(true); controls(); host.hidden = true;
    }
    async function run(operationKind) {
        if (blocked(true)) { notice(blocked(true), true); return; }
        if (!commitTimes() || !model.hasSelection) return;
        if (operationKind === "gain" && gainError(value(gain))) return;
        const operation = operationKind === "gain" ? { kind: "gain", db: value(gain) } : { kind: operationKind };
        const request = { kind, edit: model.edit(operation) }, token = generation;
        submitting = true; interaction.cancel(); controls();
        try { await apply(request); }
        catch (error) { if (token === generation) notice(error.message || String(error), true); }
        finally { if (token === generation) { submitting = false; sync(); } }
    }
    for (const [id, operation] of Object.entries(tools)) {
        $(id).dataset.toolHelp = $(id).title;
        $(id).onclick = () => run(operation);
    }
    stem.onchange = () => { choose(stem.value); view.reveal(kind); };
    for (const input of [start, end]) {
        input.addEventListener("input", () => { draft = true; controls(); });
        input.addEventListener("change", commitTimes);
        input.addEventListener("blur", () => { if (draft) commitTimes(); });
    }
    gain.addEventListener("input", controls);
    host.addEventListener("keydown", (event) => {
        if (event.key === "Escape") interaction.cancel();
        if (event.key !== "Enter" || ![start, end, gain].includes(event.target)) return;
        event.preventDefault(); event.stopPropagation();
        if (event.target !== gain) commitTimes();
    });
    view.viewport.addEventListener("click", (event) => {
        const button = event.target.closest(".s-time-lane-name");
        if (button) { choose(button.closest(".s-time-lane").dataset.kind); view.reveal(kind); }
    });
    const navigation = {
        "s-time-fit": () => model.fit(),
        "s-time-fit-selection": () => { if (commitTimes()) model.fitSelection(); },
        "s-time-zoom-in": () => model.zoom(0.5), "s-time-zoom-out": () => model.zoom(2),
        "s-time-left": () => model.pan(-1), "s-time-right": () => model.pan(1),
        "s-time-seek-selection": () => { if (commitTimes()) seek(model.start / model.rate); },
    };
    for (const [id, action] of Object.entries(navigation)) $(id).onclick = () => {
        if (blocked()) return;
        interaction.cancel(); action(); controls(); draw();
    };
    sync();
    return { sync, draw, reset };
}
