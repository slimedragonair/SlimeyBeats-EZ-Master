import { bytesLabel } from "./protocol.js";
import { serverStoragePolicy } from "./server-limits.js";
import { BROWSER_DOWNLOAD_BUDGET_BYTES, DOWNLOAD_BUDGET_NOTICE } from "./artifact-download.js";

const template = `
<details id="hybrid-studio" class="hybrid-studio">
  <summary>Server studio (optional preview)</summary>
  <div class="hybrid-content">
    <p>Local mastering stays private on this device by default. This separate, invite-only preview sends the WAV or MP3 you explicitly choose to a server. The desktop app remains the full studio for larger sessions.</p>
    <fieldset id="hybrid-connection">
      <legend>1 · Choose and connect to a server</legend>
      <label for="hybrid-endpoint">Server address (HTTPS)</label>
      <input id="hybrid-endpoint" type="url" inputmode="url" placeholder="https://api.slimedragon.net" autocomplete="off" autocapitalize="none" spellcheck="false" aria-describedby="hybrid-endpoint-note" />
      <p id="hybrid-endpoint-note" class="small">Use this site's HTTPS origin or https://api.slimedragon.net. The address starts empty. HTTP loopback is allowed only on a localhost development page with a matching connection policy.</p>
      <label for="hybrid-token">Owner or invite token</label>
      <input id="hybrid-token" type="password" autocomplete="off" autocapitalize="none" spellcheck="false" aria-describedby="hybrid-token-note" />
      <p id="hybrid-token-note" class="small">Kept only in this tab's memory while connected. Never saved in site preferences, local recovery or links. Disconnect clears it. Leaving the page disconnects; returning requires a new connection and consent.</p>
      <div class="hybrid-actions">
        <button id="hybrid-connect" type="button">Connect</button>
        <button id="hybrid-disconnect" type="button" disabled>Disconnect &amp; clear token</button>
      </div>
    </fieldset>
    <fieldset id="hybrid-options" disabled>
      <legend>2 · Choose a separate server source</legend>
      <label for="hybrid-file">WAV or MP3 for server mastering</label>
      <input id="hybrid-file" type="file" accept=".wav,.mp3,audio/wav,audio/mpeg" aria-describedby="hybrid-file-note" />
      <p id="hybrid-file-note" class="small">The server's upload limit applies here, separately from local browser decoding limits. Selecting a file does not upload it.</p>
      <div class="hybrid-grid">
        <label for="hybrid-profile">Server tonal profile<select id="hybrid-profile"></select></label>
        <label for="hybrid-format">Server output format<select id="hybrid-format" aria-describedby="hybrid-download-notice"></select></label>
        <label for="hybrid-mode">Server loudness mode<select id="hybrid-mode"><option value="streaming_safe">Streaming safe</option><option value="loud">Loud</option></select></label>
      </div>
      <p class="small">Profile loudness target · −1 dBTP ceiling · correction intensity 1. These server settings are separate from local mastering.</p>
      <p id="hybrid-download-notice" class="small"></p>
    </fieldset>
    <section aria-label="Review server upload">
      <h3>3 · Review before sending</h3>
      <dl class="hybrid-review">
        <dt>Upload destination</dt><dd id="hybrid-destination">Not connected</dd>
        <dt>Selected source</dt><dd id="hybrid-selection">No server file selected</dd>
        <dt>Server storage policy</dt><dd id="hybrid-retention">Connect to read the actual temporary-storage policy and limits.</dd>
      </dl>
      <label class="hybrid-consent" for="hybrid-consent"><input id="hybrid-consent" type="checkbox" disabled /><span>I consent to uploading this selected audio and its filename to the destination shown, for server mastering and temporary storage under the displayed policy. I have permission to send it.</span></label>
      <button id="hybrid-submit" class="primary" type="button" disabled>Upload &amp; master on server</button>
    </section>
    <section id="hybrid-job" aria-label="Server job" hidden>
      <dl class="hybrid-review">
        <dt>Job</dt><dd id="hybrid-job-id"></dd>
        <dt>Job destination</dt><dd id="hybrid-job-destination"></dd>
        <dt>Job source and settings</dt><dd id="hybrid-job-source"></dd>
        <dt>Scheduled expiry</dt><dd id="hybrid-expiry"></dd>
      </dl>
      <label for="hybrid-progress">Server job progress</label>
      <progress id="hybrid-progress" max="1" value="0"></progress>
      <p id="hybrid-progress-text" class="small"></p>
      <div class="hybrid-actions">
        <button id="hybrid-refresh" type="button" disabled>Check / resume job status</button>
        <button id="hybrid-delete" type="button" disabled>Cancel &amp; delete server files</button>
      </div>
      <p class="small">Disconnect stops this tab's requests and clears its token; server work may continue. Cancel &amp; delete sends an explicit server deletion request. An expiry time alone does not confirm deletion. Reloading or closing the tab forgets its job reference; delete server files before leaving if you need confirmed cleanup.</p>
      <ul id="hybrid-artifacts" aria-label="Server downloads"></ul>
    </section>
    <p id="hybrid-status" role="status" aria-live="polite" aria-atomic="true"></p>
  </div>
</details>`;

export function createStudioView(host) {
    host.innerHTML = template;
    const byId = (name) => host.querySelector(`#hybrid-${name}`);
    byId("download-notice").textContent = DOWNLOAD_BUDGET_NOTICE;
    let previousCaps;
    function render(session) {
        const { caps, connected, busy, job, file } = session;
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
        byId("options").disabled = !connected || !!busy;
        byId("file").disabled = !connected || !!busy || (!!job && (!session.sameServer || job.state !== "uploading"));
        for (const name of ["profile", "format", "mode"]) byId(name).disabled = !!job;
        if (job && session.source && session.sameServer) {
            for (const name of ["profile", "format", "mode"]) byId(name).value = session.source[name];
        }
        if (!file) byId("file").value = "";
        byId("consent").disabled = !connected || !file || !!busy || (!!job && (!session.sameServer || job.state !== "uploading"));
        byId("consent").checked = session.consent;
        byId("submit").disabled = !session.canSubmit;
        byId("submit").textContent = job?.state === "uploading" ? "Resume upload & master on server" :
            session.creationUncertain ? "Retry creation & upload on server" : "Upload & master on server";
        byId("destination").textContent = connected ? session.client.endpoint : "Not connected";
        byId("selection").textContent = file ? `${file.name} · ${bytesLabel(file.size)}` : "No server file selected";
        byId("retention").textContent = caps
            ? serverStoragePolicy(caps)
            : "Connect to read the actual temporary-storage policy and limits.";
        byId("job").hidden = !job;
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
