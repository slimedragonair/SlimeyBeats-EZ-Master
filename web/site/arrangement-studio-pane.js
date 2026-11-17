import { traceAdapter, sessionTrace } from "./session-trace.js";
import { createArrangementTimeline } from "./arrangement-timeline-controller.js";

// One engine owns one document, draft inspector and source transport. Hiding a
// pane pauses audition while preserving its immutable assets and Undo history.
export function createArrangementStudioPane({ mode, host, createAdapter, volume, pauseMain, changed, notice, isEditable = () => true }) {
    const displayNotice = notice;
    notice = (text, error) => { sessionTrace.message(text, error); displayNotice(text, error); };
    let adapter, controller, source, unsubscribe, opening, latest, lastNotice = "";
    function install(next) {
        if (source === next) return;
        unsubscribe?.(); source?.pause(); source = next; source.setVolume(volume());
        unsubscribe = source.subscribe(event => {
            if (event?.error) notice(event.error.message || String(event.error), true);
            changed();
        });
    }
    return {
        async initialise() {
            if (controller?.ready()) return true;
            if (opening) return opening;
            opening = (async () => {
                adapter = traceAdapter(createAdapter({ mode, beforePlay: pauseMain,
                    onChange: () => { if (controller) void controller.refresh(); },
                    onError: error => notice(error.message || String(error), true) }), mode);
                latest = await adapter.read();
                if (!latest.capabilities?.playback) throw new Error(latest.capabilities?.playback_unavailable_reason || `${mode === "server" ? "Server" : "Local"} arrangement playback is unavailable.`);
                const bands = latest.defaults?.bands;
                if (!Array.isArray(bands) || bands.length !== 7) throw new Error("The arrangement processor did not provide its seven default EQ bands.");
                controller = createArrangementTimeline(host, { adapter, defaults: () => structuredClone(bands),
                    isEditable,
                    position: () => source?.positionFrame() || 0,
                    seek: frame => source?.seekFrame(frame).catch(error => notice(error.message || String(error), true)),
                    installPreview: install,
                    onState: value => { latest = value; changed(); },
                    onNotice: (text, error) => { lastNotice = text; notice(text, error); },
                });
                if (!await controller.initialise()) throw new Error(lastNotice || "The arrangement audio preview could not be prepared.");
                return true;
            })().catch(error => {
                controller?.dispose(); controller = null; unsubscribe?.(); unsubscribe = null;
                adapter?.clear?.(); adapter = null; source = null; throw error;
            }).finally(() => { opening = null; changed(); });
            return opening;
        },
        state: () => controller?.state() || latest,
        async captureProject() {
            const before = controller.state(), ui = controller.captureUi();
            const result = await adapter.saveProject({ expectedRevision: before.revision });
            const after = controller.state();
            if (before.revision !== after.revision || before.epoch !== after.epoch || !(result?.bytes instanceof Uint8Array))
                throw new Error("The local arrangement changed during project transfer. Current work was kept.");
            return { file: new File([result.bytes], "Studio-arrangement.zip", { type: "application/zip" }), ui };
        },
        async openProject(file, ui) {
            if (!await controller.openProject(file, ui)) throw new Error(lastNotice || "The server could not continue this project. Local work was kept.");
        },
        sync: () => controller?.sync(),
        cancel: () => controller?.cancel(),
        activate: value => controller?.activate(value),
        pause: () => source?.pause(),
        source: () => source,
        importFiles: files => controller?.importFiles(files),
        get ready() { return !!controller?.ready(); },
        get busy() { return !!controller?.state().job_busy; },
        get loaded() { return !!latest?.arrangement?.lanes.some(lane => lane.clips.length); },
        dispose() { unsubscribe?.(); controller?.dispose(); adapter?.clear?.(); },
    };
}
