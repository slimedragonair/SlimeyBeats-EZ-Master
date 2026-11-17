import { wireSessionTrace } from "./session-trace.js";
wireSessionTrace(document);
// Independent entrypoint: Help/reporting is usable even if app.js or WASM fails.
import { createRecovery } from "./recovery-controller.js";
import { workspaceRecovery, subscribeRecovery } from "./recovery-bridge.js";
import { wireRecoveryDialog } from "./recovery-dialog.js";
import { wireSupportDialog } from "./support-dialog.js";
const failures = [];
window.addEventListener("error", () => {
    if (!failures.includes("page_error")) failures.push("page_error");
    document.getElementById("engine-report").hidden = false;
});
window.addEventListener("unhandledrejection", () => { if (!failures.includes("operation_rejected")) failures.push("operation_rejected"); });
function engineFailure() {
    if (!failures.includes("startup_failed")) failures.push("startup_failed");
    document.getElementById("engine-report").hidden = false;
}
document.addEventListener("engine-unavailable", engineFailure, { once: true });
// A fast initialization failure can precede this independent entrypoint.
if (document.documentElement.dataset.engineState === "failed") engineFailure();
let panel;
const recovery = createRecovery({ ...workspaceRecovery, onChange: () => {
    panel?.render();
    const paused = recovery.state.enabled && recovery.state.paused;
    document.getElementById("recovery-paused").hidden = !paused;
    document.body.classList.toggle("recovery-paused", paused);
} });
panel = wireRecoveryDialog(recovery);
document.getElementById("recovery-paused").onclick = () => panel.open();
const report = wireSupportDialog(recovery, failures);
subscribeRecovery(recovery.changed);
document.addEventListener("open-recovery", () => panel.open());
document.addEventListener("open-support-report", report);
document.getElementById("recovery-offer-open").onclick = () => {
    document.getElementById("recovery-offer").hidden = true; panel.open();
};
document.getElementById("recovery-offer-dismiss").onclick = () => { document.getElementById("recovery-offer").hidden = true; };
document.getElementById("engine-report").onclick = report;
recovery.initial.then(async () => {
    if (recovery.state.enabled && (await recovery.list()).length)
        document.getElementById("recovery-offer").hidden = false;
}).catch(() => {});
// Starting Blob reads during page teardown fails in some WebKit contexts. The
// completed debounced save is the restore point; unload/beacons promise nothing.
document.addEventListener("visibilitychange", () => { if (document.hidden) recovery.suspend(); else recovery.resume(); });
window.addEventListener("pagehide", () => recovery.suspend());
window.addEventListener("pageshow", (event) => { if (event.persisted) recovery.resume(); });
