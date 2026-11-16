import { studioTemplate } from "./studio-template.js";
import { AVAILABILITY_LABELS } from "./server-availability.js";

export function availabilityDetail(session, controller) {
    const detail = controller?.availability?.detail || "";
    if (session.job && session.unavailableJobStatus)
        return `${detail} Retry forgets unavailable access and reconnects; it does not confirm server deletion.`;
    if (!controller?.enabled || controller.availability?.state !== "online" || session.connected) return detail;
    if (controller.connecting) return `${detail} Connecting; no audio is sent.`;
    return `${detail} ${session.job
        ? "This job needs its original credential; a new guest token cannot recover it."
        : "Not connected for processing. Choose Retry to connect; no audio is sent."}`;
}

export function createStudioView(host) {
    host.innerHTML = studioTemplate;
    const byId = (name) => host.querySelector(`#hybrid-${name}`);
    function render(session, controller) {
        const enabled = !!controller?.enabled, availability = controller?.availability;
        const state = availability?.state || "idle";
        byId("enabled").checked = enabled;
        byId("studio").dataset.availability = enabled ? state : "local";
        byId("availability").textContent = !enabled ? "Local"
            : controller.connecting ? "[Connecting]" : AVAILABILITY_LABELS[state];
        const retry = enabled && !controller.connecting && !session.busy &&
            (state !== "online" || !session.connected);
        byId("check").hidden = !retry;
        byId("check").disabled = !!controller?.connecting;
        byId("status").hidden = !enabled || !session.error;
        byId("status").textContent = session.error
            ? "A server request failed. Your audio and settings are kept. Retry the connection, or use local processing. Check ⓘ for pending cleanup." : "";
        byId("availability").title = availabilityDetail(session, controller) || "Audio stays on this device.";
        byId("availability-detail").textContent = enabled ? availabilityDetail(session, controller) : "";
        byId("storage").textContent = session.caps
            ? `Server audio is deleted after the result is loaded. Interrupted jobs expire within ${Math.ceil(session.caps.privacy.retention_seconds / 60)} minutes. If deletion fails, retry it here.`
            : "Server audio uses temporary storage, with automatic expiry. Results load into this editor; export to save your work.";
        byId("destination").textContent = `Server: ${availability?.endpoint || ""}`;
        const pending = !!session.job;
        byId("job-note").hidden = !pending;
        byId("job-note").textContent = pending
            ? `Server job: ${session.job.state}. Reported expiry: ${new Date(session.job.expires_at).toLocaleString()}. Deletion is not yet confirmed.` : "";
        byId("delete").hidden = !pending;
        byId("delete").disabled = !session.sameServer || !!session.busy;
        byId("details").classList.toggle("hybrid-cleanup-pending", pending && session.error);
    }
    return { byId, render };
}
