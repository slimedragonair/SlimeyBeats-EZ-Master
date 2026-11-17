import { download } from "./view.js";
import { setOptions } from "./arrangement-timeline-fields.js";

const DELIVERY_FORMATS = [["wav24", "24-bit WAV"], ["wav16", "16-bit WAV"],
    ["wav32f", "32-bit float WAV"], ["mp3", "MP3 · 320 kbps"]];

// Explicit file actions only. Project validation/format and audio rendering
// belong to the adapter's shared core, never to the timeline DOM.
export function wireArrangementProject(host, { adapter, getState, run, adopt, pause, notice }) {
    const $ = name => host.querySelector(`[data-arr="${name}"]`);
    async function deliver(result, name, type) {
        if (result?.artifact && typeof adapter.downloadArtifact === "function") {
            await adapter.downloadArtifact(result.artifact, name || result.name); return;
        }
        const bytes = result instanceof Uint8Array ? result : result?.bytes;
        if (!(bytes instanceof Uint8Array)) throw new Error("The arrangement adapter returned no download.");
        download(bytes, name || result.name || "Studio-arrangement.zip", type || result.type || "application/zip");
    }
    $("project-save").onclick = () => {
        if ($("project-save").disabled) return;
        const expectedRevision = getState().revision;
        return run("Preparing the arrangement project…", async signal => {
            pause();
            const result = await adapter.saveProject({ expectedRevision, signal });
            await deliver(result);
            notice("Project download prepared with applied edits, source assets and Undo history. Draft fields and EQ proposals stay unapplied.");
        });
    };
    $("project-open").onclick = () => { if (!$("project-open").disabled) $("project-file").click(); };
    $("project-file").onchange = () => {
        const file = $("project-file").files[0]; $("project-file").value = "";
        if (!file || $("project-open").disabled) return;
        const expectedRevision = getState().revision;
        return run("Checking and opening the arrangement project…", async (signal, token) => {
            pause();
            const snapshot = await adapter.openProject(file, { expectedRevision, signal });
            await adopt(snapshot, token, true);
        });
    };
    $("audio-export").onclick = () => {
        if ($("audio-export").disabled) return;
        const expectedRevision = getState().revision, format = $("audio-format").value;
        return run("Rendering the whole arrangement for download…", async signal => {
            pause();
            const result = await adapter.exportAudio({ expectedRevision, format, signal,
                onProgress: (_value, text) => { if (text) notice(text); } });
            const mp3 = format === "mp3";
            await deliver(result, `Studio-arrangement.${mp3 ? "mp3" : "wav"}`, mp3 ? "audio/mpeg" : "audio/wav");
            notice(`Whole-arrangement ${mp3 ? "MP3" : "WAV"} download prepared from applied edits. Listening volume and unapplied drafts are excluded.`);
        });
    };
    $("audio-format").onchange = () => { $("mp3-note").hidden = $("audio-format").value !== "mp3"; };
    return { sync() {
        const { capabilities = {}, busy, preview_current, arrangement } = getState();
        const project = !!capabilities.project && typeof adapter.saveProject === "function" && typeof adapter.openProject === "function";
        const audio = !!capabilities.wav_export && typeof adapter.exportAudio === "function";
        const advertised = capabilities.export_formats || ["wav24", "wav16", "wav32f", ...(capabilities.mp3_export ? ["mp3"] : [])];
        const formats = DELIVERY_FORMATS.filter(([value]) => advertised.includes(value));
        const selected = formats.some(([value]) => value === $("audio-format").value) ? $("audio-format").value
            : formats.some(([value]) => value === "wav24") ? "wav24" : formats[0]?.[0] || "";
        setOptions($("audio-format"), formats, selected);
        $("mp3-note").hidden = selected !== "mp3";
        $("project-actions").hidden = !project;
        $("audio-actions").hidden = !audio;
        $("file-actions").hidden = !project && !audio;
        $("persistence-note").hidden = project;
        $("project-save").disabled = $("project-open").disabled = $("project-file").disabled = !!busy;
        const hasAudio = arrangement?.lanes.some(lane => lane.clips.length);
        $("audio-export").disabled = !!busy || !preview_current || !hasAudio || !formats.length;
        $("audio-format").disabled = !!busy;
    } };
}
