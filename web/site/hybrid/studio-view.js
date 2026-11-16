import { bytesLabel } from "./protocol.js";
import { serverStoragePolicy, temporaryStorageSummary } from "./server-limits.js";
import { BROWSER_DOWNLOAD_BUDGET_BYTES, DOWNLOAD_BUDGET_NOTICE } from "./artifact-download.js";

import { studioTemplate } from "./studio-template.js";
import { AVAILABILITY_LABELS } from "./server-availability.js";
import { PUBLIC_API_ORIGIN, serverEndpointHelp } from "./endpoint-policy.js";

export function availabilityDetail(session, controller) {
    const availability = controller?.availability;
    const detail = availability?.detail || "";
    if (session.job && session.unavailableJobStatus)
        return `${detail} The remembered job is unavailable. Use Forget unavailable job in its job panel to clear local server access, then reconnect explicitly. Forgetting does not confirm server deletion.`;
    if (!controller?.enabled || availability?.state !== "online" || session.connected) return detail;
    if (controller.connecting) return `${detail} Connecting for processing; no audio is sent.`;
    return `${detail} ${session.job
        ? "This tab is disconnected from its remembered job. Reconnect with the original credential or an operator in advanced settings; a new guest token cannot recover it."
        : "Not connected for processing yet. Choose Check & connect to request guest access. This connection step sends no audio."}`;
}

export function createStudioView(host) {
    host.innerHTML = studioTemplate;
    const byId = (name) => host.querySelector(`#hybrid-${name}`);
    byId("endpoint").value = PUBLIC_API_ORIGIN;
    byId("endpoint").placeholder = PUBLIC_API_ORIGIN;
    byId("endpoint-note").textContent = serverEndpointHelp();
    byId("download-notice").textContent = DOWNLOAD_BUDGET_NOTICE;
    let previousCaps;
    function render(session, controller) {
        const { caps, connected, busy, job, file } = session;
        const enabled = !!controller?.enabled;
        const availability = controller?.availability;
        byId("enabled").checked = enabled;
        byId("panel").hidden = !enabled;
        byId("operator").hidden = !enabled && !job && !connected;
        byId("mode-note").textContent = enabled
            ? "Server processing selected. Review below before uploading. Turning this off keeps access to existing server jobs. No automatic local fallback."
            : "Local processing is selected. Turning this on checks the server; it never uploads or masters audio.";
        byId("availability").textContent = AVAILABILITY_LABELS[availability?.state || "idle"];
        byId("studio").dataset.availability = availability?.state || "idle";
        byId("availability-detail").textContent = availabilityDetail(session, controller);
        byId("availability-detail").hidden = !enabled && !job && availability?.state === "idle";
        byId("check").textContent = controller?.connecting ? "Connecting…"
            : enabled && !connected && !job && (availability?.state === "online" || session.forgottenJob) ? "Check & connect" : "Check / retry server";
        byId("check").disabled = !!controller?.connecting;
        if (caps !== previousCaps) {
            previousCaps = caps;
            for (const [name, values] of [["profile", caps?.profiles || []], ["format", (caps?.formats || [])
                .filter((value) => ["wav24", "mp3"].includes(value)).map((id) => ({ id, label: id === "wav24" ? "24-bit WAV" : "MP3" }))]]) {
                byId(name).replaceChildren(...values.map(({ id, label }) => {
                    const option = host.ownerDocument.createElement("option");
                    option.value = id; option.textContent = label; return option;
                }));
            }
        }
        byId("endpoint").disabled = connected || !!busy;
        byId("token").disabled = connected || !!busy;
        byId("connect").disabled = connected || !!busy;
        byId("disconnect").disabled = !connected && !busy;
        byId("options").disabled = !enabled || !!busy;
        byId("file").disabled = !!busy || (!!job && (!session.sameServer || job.state !== "uploading"));
        for (const name of ["profile", "format", "mode"]) byId(name).disabled = !connected || !!job;
        if (job && session.source && session.sameServer) {
            for (const name of ["profile", "format", "mode"]) byId(name).value = session.source[name];
        }
        if (!file) byId("file").value = "";
        byId("consent").disabled = !enabled || !session.available || !connected || !!session.unavailableJobStatus || !file || !!busy || (!!job && (!session.sameServer || job.state !== "uploading"));
        byId("consent").checked = session.consent;
        byId("submit").disabled = !session.canSubmit;
        byId("submit").textContent = job?.state === "uploading" ? "Resume upload & master on server" :
            session.creationUncertain ? "Retry creation & upload on server" : "Upload & master on server";
        byId("destination").textContent = connected ? session.client.endpoint : availability?.endpoint || byId("endpoint").value;
        const selected = connected ? file : controller?.mainFile || file;
        byId("selection").textContent = selected ? `${selected.name} · ${bytesLabel(selected.size)}${!connected ? " · awaiting connection and validation" : ""}` : "No server file selected";
        byId("current-source").textContent = selected ? `Server source: ${selected.name} · ${bytesLabel(selected.size)}` : "No server source selected.";
        byId("retention").textContent = caps
            ? temporaryStorageSummary(caps)
            : "Connect to read the actual temporary-storage policy and limits.";
        byId("limit-details").textContent = caps ? serverStoragePolicy(caps) : "Connect to read server limits.";
        byId("job").hidden = !job;
        byId("unavailable-job").hidden = !job || !session.unavailableJobStatus;
        byId("unavailable-note").textContent = session.unavailableJobStatus
            ? `This job is unavailable to this session (HTTP ${session.unavailableJobStatus}). You can retry access, or forget it locally to start again without reloading. Forgetting clears its job reference and credential; it does not request or confirm server deletion. The previous reported expiry still governs any remaining files. Local audio, settings and source stay intact.` : "";
        byId("forget").disabled = !session.canForgetUnavailableJob;
        byId("forgotten-note").hidden = !session.forgottenJob;
        const forgottenText = session.forgottenJob
            ? `Most recently forgotten job at ${session.forgottenJob.endpoint}: no server deletion was requested or confirmed. Its previous reported expiry (${new Date(session.forgottenJob.expires_at).toISOString()}) still governs any remaining server files.` : "";
        if (byId("forgotten-note").textContent !== forgottenText) byId("forgotten-note").textContent = forgottenText;
        if (job) {
            byId("job-id").textContent = `${job.id} · ${job.state}`;
            byId("job-destination").textContent = session.jobEndpoint;
            const source = session.source;
            byId("job-source").textContent = source
                ? `${source.filename} · ${bytesLabel(source.bytes)} · ${source.profile} · ${source.format} · ${source.mode}` : "Unknown";
            byId("expiry").textContent = `${new Date(job.expires_at).toLocaleString()} (${new Date(job.expires_at).toISOString()}) · reported by server`;
            byId("progress").value = job.state === "uploading" ? job.uploaded_bytes / job.total_bytes : job.progress;
            byId("progress-text").textContent = job.state === "uploading"
                ? `Uploaded ${bytesLabel(job.uploaded_bytes)} of ${bytesLabel(job.total_bytes)}`
                : `${job.state} · ${Math.round(job.progress * 100)}%`;
        }
        byId("refresh").disabled = !job || !session.sameServer || !!busy;
        byId("delete").disabled = !job || !session.sameServer || busy === "Deleting server files…";
        renderArtifacts(session);
        byId("status").textContent = session.message;
        byId("status").hidden = !enabled && !job && !session.forgottenJob;
        byId("status").classList.toggle("hybrid-error", session.error);
        byId("studio").dataset.state = connected ? (busy ? "busy" : "connected") : (busy ? "connecting" : "disconnected");
    }
    // Preserve focused download buttons during job/status updates.
    let artifactKey = "";
    function renderArtifacts(session) {
        const artifacts = session.job?.state === "completed" ? session.job.artifacts : [];
        const nextKey = JSON.stringify(artifacts);
        if (nextKey !== artifactKey) {
            artifactKey = nextKey;
            byId("artifacts").replaceChildren(...artifacts.map((artifact) => {
                const item = host.ownerDocument.createElement("li"), button = host.ownerDocument.createElement("button");
                button.type = "button";
                button.dataset.artifactId = artifact.id;
                button.id = `hybrid-download-${artifact.id}`;
                button.textContent = `Download ${artifact.name} · ${bytesLabel(artifact.bytes)}`;
                button.dataset.overBudget = String(artifact.bytes > BROWSER_DOWNLOAD_BUDGET_BYTES);
                if (button.dataset.overBudget === "true") {
                    button.textContent += " · exceeds 128 MiB download budget";
                    button.title = DOWNLOAD_BUDGET_NOTICE;
                }
                item.append(button); return item;
            }));
        }
        for (const button of byId("artifacts").querySelectorAll("button"))
            button.disabled = !session.sameServer || !!session.busy || button.dataset.overBudget === "true";
    }
    return { byId, render };
}
