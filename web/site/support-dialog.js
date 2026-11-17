import { sessionTrace } from "./session-trace.js";
import { wireDialogFocus } from "./dialog-focus.js";
import { download } from "./view.js";
import { buildSupportReport } from "./support-report.js";
import { workspaceRecovery } from "./recovery-bridge.js";
const $ = (id) => document.getElementById(id);
export function wireSupportDialog(recovery, failures) {
    const dialog = $("support-dialog"), modal = wireDialogFocus(dialog);
    let preview = "";
    const invalidate = () => {
        preview = ""; $("support-preview").textContent = "Choose Preview report to inspect exactly what you would share.";
        $("support-copy").disabled = $("support-download").disabled = true;
        $("support-status").textContent = "";
    };
    $("support-description").oninput = $("support-area").onchange = invalidate;
    $("support-build").onclick = () => {
        preview = buildSupportReport($("support-description").value, $("support-area").value,
            workspaceRecovery.context(), failures, recovery.state);
        $("support-preview").textContent = preview;
        $("support-copy").disabled = $("support-download").disabled = false;
        $("support-status").textContent = "Review the exact text below before copying or downloading. Nothing is sent automatically.";
    };
    $("support-copy").onclick = async () => {
        try { await navigator.clipboard.writeText(preview); $("support-status").textContent = "Reviewed report copied. You decide where to share it."; }
        catch { $("support-status").textContent = "Clipboard access is blocked. Select the preview text or download the report instead."; }
    };
    $("support-download").onclick = () => download(preview, "SlimeyBeats-support-report.txt", "text/plain");
    $("support-close").onclick = () => dialog.close();
    const traceHelp = document.createElement("p");
    traceHelp.textContent = "Session trace: up to 1,000 JSON events kept only in this tab's memory. Records control names, operation outcomes and known app messages, file MD5, size and detected encoding; not audio, filenames, tokens or custom text. File hashes can identify matching content; review before sharing. Unknown messages are omitted. Nothing is uploaded automatically; reloading clears the trace.";
    const tracePreview = document.createElement("pre"); tracePreview.hidden = true;
    const previewTrace = document.createElement("button"); previewTrace.id = "trace-preview"; previewTrace.type = "button"; previewTrace.textContent = "Preview session JSON trace";
    const saveTrace = document.createElement("button"); saveTrace.id = "trace-download"; saveTrace.type = "button"; saveTrace.textContent = "Download reviewed JSONL"; saveTrace.disabled = true;
    const clearTrace = document.createElement("button"); clearTrace.id = "trace-clear"; clearTrace.type = "button"; clearTrace.textContent = "Clear session trace";
    let reviewedTrace = "";
    previewTrace.onclick = () => { reviewedTrace = sessionTrace.exportJsonl(); tracePreview.textContent = reviewedTrace; tracePreview.hidden = false; saveTrace.disabled = false; };
    saveTrace.onclick = () => download(reviewedTrace, "SlimeyBeats-session-trace.jsonl", "application/x-ndjson");
    clearTrace.onclick = () => { sessionTrace.clear(); reviewedTrace = ""; tracePreview.textContent = ""; tracePreview.hidden = true; saveTrace.disabled = true; };
    dialog.append(traceHelp, previewTrace, saveTrace, clearTrace, tracePreview);
    invalidate();
    return () => { modal.open(); $("support-title").focus({ preventScroll: true }); };
}
