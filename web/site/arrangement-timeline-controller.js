import { arrangementTimelineTemplate } from "./arrangement-timeline-template.js";
import { ArrangementTimelineState, frameSeconds } from "./arrangement-timeline-state.js";
import { createArrangementTimelineView } from "./arrangement-timeline-view.js";
import { wireArrangementTimelineInteraction } from "./arrangement-timeline-interaction.js";
import { wireArrangementInspector } from "./arrangement-timeline-inspector.js";
import { wireArrangementImport } from "./arrangement-timeline-import.js";
import { wireArrangementEq } from "./arrangement-timeline-eq.js";
import { wireArrangementProject } from "./arrangement-timeline-project.js";
import { setText } from "./arrangement-timeline-fields.js";

const MAX_ENVELOPES = 256;
const MAX_ENVELOPE_REQUESTS = 4;

// The adapter owns decoding, edits/history, DSP and playable sources. This
// controller owns UI state and never mixes PCM or dispatches to the stem bank.
export function createArrangementTimeline(host, {
    adapter, defaults, installPreview, position, seek, onState = () => {}, onNotice = () => {}, isEditable = () => true,
}) {
    host.innerHTML = arrangementTimelineTemplate;
    const $ = name => host.querySelector(`[data-arr="${name}"]`), model = new ArrangementTimelineState();
    const envelopes = new Map(), pendingEnvelopes = new Map();
    const envelopeLru = [];
    let snapshot = { revision: 0, arrangement: null, assets: [], history: {}, capabilities: {} };
    let busy = false, disposed = false, active = false, exposed = false, generation = 0, abort, previewCurrent = false, frame, envelopeWindow = "", refreshQueued = false, transport = null, libraryGeneration = 0;
    function state() { return { ...snapshot, job_busy: busy, busy: busy || !isEditable(), preview_current: previewCurrent, playable: exposed && usable() && isEditable() }; }
    function notice(text, error = false) {
        if (disposed) return;
        setText($("status"), text); $("status").classList.toggle("error", error); onNotice(text, error);
    }
    function usable() {
        return !!snapshot.capabilities?.playback && typeof installPreview === "function" &&
            typeof position === "function" && typeof seek === "function" &&
            typeof adapter.edit === "function" && typeof adapter.previewSource === "function";
    }
    function accept(value, replaceProject = false) {
        if (!value?.arrangement || !Array.isArray(value.assets) || !Number.isSafeInteger(value.revision))
            throw new Error("The arrangement adapter returned incomplete project metadata.");
        if (value.revision < snapshot.revision) throw new Error("A newer arrangement revision is already displayed. Your current work was kept.");
        replaceProject ||= !!snapshot.arrangement && value.epoch !== snapshot.epoch;
        interaction.cancel();
        if (replaceProject) { model.reset(); ++libraryGeneration; envelopes.clear(); envelopeLru.length = 0; }
        model.configure(value.arrangement, value.revision, value.assets);
        snapshot = value; previewCurrent = false;
        if (replaceProject) { inspector.reset(); importer.reset(); eq.reset(); }
    }
    async function preview(token) {
        const expected = snapshot.revision, source = await adapter.previewSource({ expectedRevision: expected, signal: abort?.signal });
        if (disposed || token !== generation || expected !== snapshot.revision) return;
        const latest = await adapter.read({ signal: abort?.signal });
        if (disposed || token !== generation || latest.revision !== expected) return;
        snapshot = latest;
        const prepared = latest.preview;
        if (!source || source.kind !== "arrangement" || prepared?.pending ||
            prepared?.prepared_revision !== expected || source.identity !== prepared.identity)
            throw new Error("The edit is saved, but its audio preview is not ready. Playback still uses the earlier revision.");
        if (["play", "pause", "stop", "seek", "seekFrame", "choose", "setVolume", "position", "positionFrame", "subscribe"].some(name => typeof source[name] !== "function"))
            throw new Error("The arrangement adapter did not return a functional playback transport.");
        if (source.sample_rate !== model.rate || source.channels !== model.arrangement.channels || source.duration_frames !== model.frames)
            throw new Error("The arrangement preview layout does not match the current project. Existing playback was kept.");
        await installPreview(source, state());
        transport = source;
        if (!disposed && token === generation && expected === snapshot.revision) previewCurrent = true;
    }
    async function run(label, action) {
        if (busy || disposed) return false;
        if (exposed && !isEditable()) { notice("This local arrangement is kept. Turn Server Off to continue editing it."); return false; }
        busy = true; abort = new AbortController(); const token = ++generation;
        notice(label);
        try { sync(); onState(state()); return await action(abort.signal, token); }
        catch (error) {
            if (!disposed && token === generation && error.name !== "AbortError") notice(error.message || String(error), true);
            return false;
        } finally {
            if (!disposed && token === generation) {
                busy = false; abort = null; sync(); onState(state());
                if (refreshQueued) { refreshQueued = false; Promise.resolve().then(refresh); }
            }
        }
    }
    async function adopt(value, token, replaceProject = false) {
        if (disposed || token !== generation) return false;
        accept(value, replaceProject); sync();
        try { await preview(token); }
        catch (error) { notice(error.message || String(error), true); onState(state()); return true; }
        if (!disposed && token === generation) notice("Arrangement preview updated. Source audio is unchanged; no audio was exported.");
        return true;
    }
    function command(edits) {
        if (!exposed || !usable()) { notice("The arrangement audio adapter is not ready.", true); return Promise.resolve(false); }
        const expectedRevision = snapshot.revision;
        return run("Updating the arrangement preview…", async (signal, token) =>
            adopt(await adapter.edit(edits, { expectedRevision, signal }), token));
    }
    function history(redo) {
        if (!exposed || !usable() || !(redo ? snapshot.history?.can_redo : snapshot.history?.can_undo)) return Promise.resolve(false);
        const expectedRevision = snapshot.revision;
        return run(redo ? "Restoring the next arrangement revision…" : "Restoring the previous arrangement revision…", async (signal, token) =>
            adopt(await adapter[redo ? "redo" : "undo"]({ expectedRevision, signal }), token));
    }
    function refresh() {
        if (disposed) return Promise.resolve(false);
        if (busy) { refreshQueued = true; return Promise.resolve(false); }
        return run("Checking the arrangement preview…", async (signal, token) => {
            const value = await adapter.read({ signal });
            const previous = snapshot.preview, next = value.preview;
            if (value.revision === snapshot.revision && previous?.identity === next?.identity &&
                previous?.prepared_revision === next?.prepared_revision && previous?.pending === next?.pending) {
                snapshot = value; sync(); return true;
            }
            return adopt(value, token);
        });
    }
    function getEnvelope(assetId, first, last, bins) {
        const library = libraryGeneration;
        const windowKey = JSON.stringify([library, snapshot.revision, model.left, model.right, model.selectedClip, Math.floor(view.viewport.scrollTop / 132)]);
        if (envelopeWindow !== windowKey) { envelopeWindow = windowKey; envelopes.clear(); envelopeLru.length = 0; }
        const key = JSON.stringify([library, assetId, first, last, bins]);
        if (envelopes.has(key)) return envelopes.get(key);
        if (pendingEnvelopes.has(key) || pendingEnvelopes.size >= MAX_ENVELOPE_REQUESTS || !usable() || !snapshot.capabilities?.waveforms) return null;
        const request = Promise.resolve().then(() => adapter.waveform(assetId, { start_frame: first, end_frame: last, bins }));
        pendingEnvelopes.set(key, request);
        Promise.resolve(request).then(envelope => {
            if (disposed || library !== libraryGeneration || !snapshot.assets.some(asset => asset.id === assetId)) return;
            if (!Array.isArray(envelope?.peaks) || envelope.peaks.some(pair => !Array.isArray(pair) || pair.length !== 2 || !pair.every(Number.isFinite)))
                throw new Error("The source waveform metadata is invalid.");
            envelopes.set(key, envelope);
            envelopeLru.push(key);
            while (envelopeLru.length > MAX_ENVELOPES) {
                const old = envelopeLru.shift();
                envelopes.set(old, { deferred: true });
            }
        }).catch(error => {
            if (!disposed) envelopes.set(key, { error: error.message || String(error) });
        }).finally(() => {
            pendingEnvelopes.delete(key);
            if (!disposed) view.render();
        });
        return null;
    }
    function repaint(reveal = false) {
        sync(); if (reveal) view.reveal(model.selectedLane);
    }
    const view = createArrangementTimelineView(host, { getEnvelope,
        selectLane: id => { if (!busy) { model.selectLane(id); repaint(); } },
        selectClip: id => { if (!busy) { model.selectClip(id); repaint(); } },
        toggleLane: (id, field) => {
            const lane = model.lane(id);
            if (!busy && lane) void command({ kind: "set_lane", lane_id: id, [field]: !lane[field] });
        },
    });
    const inspector = wireArrangementInspector(host, { model: () => model, getState: state, command,
        position: () => Math.round(position?.() || 0), repaint, notice });
    const interaction = wireArrangementTimelineInteraction(view, { model: () => model, allowed: () => !busy && exposed && usable() && isEditable(),
        sourceFrames: id => snapshot.assets.find(asset => asset.id === id)?.frames,
        repaint, selectLane: id => model.selectLane(id), selectClip: id => model.selectClip(id), seek: value => seek?.(value),
        commit: edit => void command(edit), nudge: inspector.nudge,
        shortcut: action => action === "undo" || action === "redo" ? void history(action === "redo") : void inspector.action(action),
    });
    const importer = wireArrangementImport(host, {
        getState: state, notice,
        review: (files, options) => run("Reviewing tracks to add…", signal => adapter.reviewImport(files, { ...options, signal })),
        add: (review, selection) => {
            const expectedRevision = snapshot.revision;
            return run("Adding the selected tracks…", async (signal, token) =>
                adopt(await adapter.addImport(review, selection, { expectedRevision, signal }), token));
        },
        discard: review => run("Discarding the pending track selection…", async signal => {
            if (typeof adapter.discardImport !== "function") throw new Error("The adapter cannot clear this pending import yet. Existing tracks are kept.");
            await adapter.discardImport(review, { signal }); return true;
        }),
    });
    const userRun = (label, action) => exposed && usable() ? run(label, action) : Promise.resolve(false);
    const eq = wireArrangementEq(host, { model: () => model, getState: state, adapter, defaults, command, run: userRun, repaint, notice });
    const project = wireArrangementProject(host, { adapter, getState: state, run: userRun, adopt, pause: () => transport?.pause(), notice });
    function sync() {
        if (disposed) return;
        const available = exposed && usable(); $("workspace").hidden = !available;
        const locked = busy || !isEditable();
        $("add-zip").hidden = !snapshot.capabilities?.import_zip;
        setText($("layout"), `${snapshot.mode === "server" ? "Server" : "Local"} arrangement · ${model.rate.toLocaleString()} Hz · ${model.arrangement?.channels === 1 ? "Mono" : "Stereo"} · ${model.lanes.length} tracks`);
        $("server-buffering").hidden = snapshot.mode !== "server";
        for (const name of ["add-files", "add-zip", "mode-move", "mode-select"]) $(name).disabled = locked || !available;
        for (const mode of ["move", "select"]) $("mode-" + mode).setAttribute("aria-pressed", String(model.mode === mode));
        $("undo").disabled = locked || !available || !snapshot.history?.can_undo;
        $("redo").disabled = locked || !available || !snapshot.history?.can_redo;
        $("fit-selection").disabled = !model.hasRange && !model.clip();
        setText($("undo"), snapshot.history?.undo_label ? `Undo ${snapshot.history.undo_label}` : "Undo");
        setText($("redo"), snapshot.history?.redo_label ? `Redo ${snapshot.history.redo_label}` : "Redo");
        setText($("selection"), model.hasRange ? `Selection ${frameSeconds(model.start, model.rate)}–${frameSeconds(model.end, model.rate)} s · frames ${model.start}–${model.end}, end exclusive`
            : "No time range selected. Choose a clip or drag a range.");
        view.sync(model, { assets: snapshot.assets, busy: locked }); inspector.sync(); importer.sync(); eq.sync(); project.sync();
    }
    for (const mode of ["move", "select"]) $("mode-" + mode).onclick = () => { if (!busy) { interaction.cancel(); model.mode = mode; sync(); } };
    const navigation = { fit: () => model.fit(), "fit-selection": () => model.fitSelection(), "zoom-in": () => model.zoom(0.5), "zoom-out": () => model.zoom(2), earlier: () => model.pan(-1), later: () => model.pan(1) };
    for (const [name, action] of Object.entries(navigation)) $(name).onclick = () => { interaction.cancel(); action(); sync(); };
    $("undo").onclick = () => history(false); $("redo").onclick = () => history(true);
    function tick() {
        if (disposed || !active) return;
        view.position(Math.round(position?.() || 0)); frame = requestAnimationFrame(tick);
    }
    const styleUrl = new URL("./arrangement-timeline.css", import.meta.url);
    styleUrl.search = new URL(import.meta.url).search;
    const style = styleUrl.href;
    if (![...document.querySelectorAll('link[rel="stylesheet"]')].some(link => link.href === style)) {
        const link = document.createElement("link"); link.rel = "stylesheet"; link.href = style; document.head.append(link);
    }
    return {
        async initialise() {
            return run("Opening the arrangement workspace…", async (signal, token) => {
                const value = await adapter.read({ signal });
                if (disposed || token !== generation) return false;
                accept(value);
                if (!usable()) { notice("Arrangement playback is not available from this adapter yet."); return false; }
                await preview(token); exposed = true; sync(); return true;
            });
        },
        activate(value) { active = !!value; cancelAnimationFrame(frame); if (active) { sync(); frame = requestAnimationFrame(tick); } else { interaction.cancel(); transport?.pause(); } },
        sync,
        captureUi() {
            interaction.cancel();
            const selection = Object.fromEntries(["selectedLane", "selectedClip", "start", "end", "left", "right", "mode", "scope", "rangeTarget"].map(key => [key, model[key]]));
            return structuredClone({ selection, inspector: inspector.capture(), eq: eq.capture(),
                position_frame: transport?.positionFrame() || 0, side: transport?.side || "after", delivery_format: $("audio-format").value,
                details: [...host.querySelectorAll("details[data-arr]")].map(element => [element.dataset.arr, element.open]) });
        },
        openProject(file, ui) {
            if (!snapshot.capabilities.project || typeof adapter.openProject !== "function")
                throw new Error("This server cannot continue an existing arrangement project yet. Local work was kept.");
            const expectedRevision = snapshot.revision;
            return run("Continuing the current arrangement on the server…", async (signal, token) => {
                const value = await adapter.openProject(file, { expectedRevision, signal });
                const accepted = await adopt(value, token, true);
                if (!accepted || !previewCurrent) throw new Error("The transferred project is saved, but its server preview is not ready. Local work was kept.");
                Object.assign(model, ui.selection);
                inspector.restore(ui.inspector); eq.restore(ui.eq);
                if (transport.side !== ui.side) await transport.choose(ui.side);
                if (ui.position_frame) await transport.seekFrame(ui.position_frame);
                if ([...$("audio-format").options].some(option => option.value === ui.delivery_format)) $("audio-format").value = ui.delivery_format;
                for (const [name, open] of ui.details) { const element = $(name); if (element?.tagName === "DETAILS") element.open = open; }
                sync(); return true;
            });
        },
        cancel() { if (busy && abort) { abort.abort(new DOMException("Arrangement operation cancelled", "AbortError")); notice("Cancelling this arrangement operation. Applied edits and source assets are kept."); } },
        importFiles: files => importer.choose([...files]),
        refresh,
        state,
        ready: () => exposed && usable(),
        metrics: () => ({ ...view.metrics(), cached_envelopes: envelopes.size, pending_envelopes: pendingEnvelopes.size }),
        dispose() { disposed = true; active = false; ++generation; abort?.abort(); transport?.pause(); cancelAnimationFrame(frame);
            interaction.dispose(); inspector.dispose(); importer.dispose(); view.dispose(); eq.dispose(); envelopes.clear(); pendingEnvelopes.clear(); host.replaceChildren(); },
    };
}
