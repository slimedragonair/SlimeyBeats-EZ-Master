import { createArrangementStudioPane } from "./arrangement-studio-pane.js";
import { serverProcessing } from "./hybrid/main-controls.js";
import { time } from "./view.js";

export const ARRANGEMENT_UI_ENABLED = true;

// Server On selects the destination of the existing Arrange tracks action.
// Each engine retains its own document; switching never uploads local assets.
export function createArrangementStudioWorkspace({ createAdapter, legacyPlayback, pauseMain, legacyBusy, notice, changed }) {
    const $ = name => document.getElementById(`s-${name}`), panes = new Map();
    let selected = false, mode = "local", active = false, selecting = false, generation = 0;
    let requested = serverProcessing.enabled ? "server" : "local";
    const current = () => panes.get(mode);
    function pane(engine) {
        if (!panes.has(engine)) panes.set(engine, createArrangementStudioPane({ mode: engine,
            host: $(engine === "local" ? "arrangement-page" : "arrangement-server-page"), createAdapter,
            volume: () => Number($("volume").value), pauseMain,
            isEditable: () => engine === (serverProcessing.enabled ? "server" : "local"),
            changed: () => changed(),
            notice: (text, error) => { if (selected && mode === engine) notice(text, error); },
        }));
        return panes.get(engine);
    }
    function adopt(engine) {
        current()?.activate(false); current()?.pause(); legacyPlayback.pause(); pauseMain();
        mode = engine; selected = true; pane(engine).source()?.setVolume(Number($("volume").value));
        pane(engine).activate(active); sync(); changed();
    }
    async function choose() {
        if (legacyBusy() || current()?.busy || selecting) return;
        const engine = serverProcessing.enabled ? "server" : "local", token = ++generation;
        selecting = true; const previous = current()?.state();
        notice(`Opening the ${engine} arrangement workspace…`); sync(); changed();
        try {
            await pane(engine).initialise();
            if (token !== generation || engine !== (serverProcessing.enabled ? "server" : "local")) return;
            const latest = current()?.state();
            if (selected && previous && (previous.revision !== latest?.revision || previous.epoch !== latest?.epoch)) {
                notice("Your current arrangement edit was kept. Choose Arrange tracks again to switch workspaces."); return;
            }
            adopt(engine);
            notice(`${engine === "server" ? "Server" : "Local"} arrangement ready. Source assets and project history stay separate from the other workspace.`);
        } catch (error) {
            if (token === generation) notice(`${engine === "server" ? "Server" : "Local"} arrangement unavailable: ${error.message || String(error)} Current local work is preserved.`, true);
        } finally { if (token === generation) selecting = false; sync(); changed(); }
    }
    function legacy() {
        if (legacyBusy() || current()?.busy || selecting) return;
        ++generation; current()?.activate(false); current()?.pause(); selected = false;
        $("before").querySelector(".compare-long").textContent = "A · Before";
        $("before").setAttribute("aria-label", "A · Before"); sync(); changed();
    }
    function engineChanged() {
        const next = serverProcessing.enabled ? "server" : "local";
        if (next === requested) return;
        requested = next; ++generation; selecting = false;
        if (selected && mode === "server" && next === "local") {
            current().activate(false); current().pause(); selected = false;
            if (panes.get("local")?.ready) adopt("local");
            notice("Local work is available again. The server arrangement is retained and paused.");
        } else if (selected && mode !== next) {
            current()?.pause();
            notice("Your local arrangement is kept and paused. Choose Arrange tracks · Server to edit on the server, or turn Server Off to continue locally.");
        }
    }
    function syncTransport() {
        const source = current()?.source(), value = current()?.state();
        const frames = source?.side_duration_frames ?? source?.duration_frames ?? 0;
        const locked = !!value?.busy || !value?.preview_current, usable = !!source && frames > 0;
        $("play").disabled = source?.playing ? false : locked || !usable;
        $("stop").disabled = !source || (!source.playing && !usable);
        $("seek").disabled = locked || !usable;
        const comparable = !!source && (usable || source.duration_frames > 0 || value?.history?.can_undo);
        for (const name of ["before", "after"]) $(name).disabled = locked || !comparable;
        const seconds = source?.position() || 0, duration = frames / (source?.sample_rate || 48000);
        $("play").textContent = source?.playing ? "Pause" : "Play";
        for (const side of ["before", "after"]) $(side).classList.toggle("selected", source?.side === side);
        $("clock").textContent = `${time(seconds)} / ${time(duration)}`;
        $("seek").max = duration || 1; $("seek").value = seconds;
        $("quick-mix-row").hidden = true;
        $("compare-state").textContent = "A Before last edit · B Arrangement preview · levels are not matched";
        $("before").querySelector(".compare-long").textContent = "A · Before last edit";
        $("before").setAttribute("aria-label", "A · Before last edit");
        $("after").querySelector(".compare-long").textContent = "B · Arrangement";
        $("after").setAttribute("aria-label", "B · Arrangement preview");
    }
    function sync() {
        engineChanged();
        $("workspace-views").hidden = !serverProcessing.enabled && ![...panes.values()].some(value => value.ready);
        $("arrange-entry").textContent = `Arrange tracks · ${serverProcessing.enabled ? "Server" : "Local"}`;
        $("arrange-entry").disabled = $("legacy-entry").disabled = legacyBusy() || !!current()?.busy || selecting;
        const requestedMode = serverProcessing.enabled ? "server" : "local";
        for (const [name, value] of [["arrange-entry", selected && mode === requestedMode], ["legacy-entry", !selected]]) {
            $(name).classList.toggle("selected", value); $(name).setAttribute("aria-pressed", String(value));
        }
        $("legacy-workspace").hidden = selected;
        $("arrangement-page").hidden = !selected || mode !== "local";
        $("arrangement-server-page").hidden = !selected || mode !== "server";
        $("legacy-waves").hidden = selected;
        $("playback-details").querySelector("summary").textContent = selected ? "Volume & seeking" : "Waveforms, volume & seeking";
        if (selected) { $("cancel").disabled = true; $("close").disabled = !!current()?.busy; current()?.sync(); syncTransport(); }
    }
    $("arrange-entry").onclick = choose; $("legacy-entry").onclick = legacy;
    return {
        async initialise() {
            if (serverProcessing.enabled) { sync(); return true; }
            try { await pane("local").initialise(); sync(); return true; }
            catch (error) { notice(`Local arrangement unavailable: ${error.message || String(error)}`, true); return false; }
        },
        sync,
        activate(value) { active = !!value; current()?.activate(active && selected); if (!active) for (const value of panes.values()) value.pause(); },
        pause() { for (const value of panes.values()) value.pause(); },
        transport: () => selected ? current()?.source() : null,
        importFiles: files => selected ? current()?.importFiles(files) : false,
        get selected() { return selected; },
        get busy() { return selecting || [...panes.values()].some(value => value.busy); },
        get loaded() { return [...panes.values()].some(value => value.loaded); },
        dispose() { for (const value of panes.values()) value.dispose(); },
    };
}
