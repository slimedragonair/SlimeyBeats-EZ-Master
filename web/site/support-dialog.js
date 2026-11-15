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
    invalidate();
    return () => { modal.open(); $("support-title").focus({ preventScroll: true }); };
}
