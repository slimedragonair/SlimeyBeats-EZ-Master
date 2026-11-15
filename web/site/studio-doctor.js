import { DoctorState } from "./studio-doctor-state.js";
import { issueTime } from "./studio-doctor-format.js";
import { download } from "./view.js";

const COLORS = { clipping: "var(--doctor-clipping)", silence: "var(--doctor-silence)", possible_ending: "var(--doctor-ending)", low_mid_buildup: "var(--doctor-low-mids)" };
const measured = value => JSON.stringify(value, null, 2);
const setText = (node, value) => { if (node.textContent !== value) node.textContent = value; };

export function wireStudioDoctor({ client, getState, job, mutate, seek, draw, sync }) {
    const $ = id => document.getElementById(`s-doctor-${id}`), state = new DoctorState();
    const target = () => document.getElementById("s-target");
    function status(message) { setText($("status"), message); }
    function select(id) {
        if (getState().busy || !state.report) return;
        state.selected = id;
        const issue = state.issue;
        if (!issue) return;
        for (const node of document.querySelectorAll("[data-doctor-issue]")) {
            const selected = node.dataset.doctorIssue === id;
            node.setAttribute("aria-pressed", String(selected));
            node.classList.toggle("selected", selected);
        }
        seek(issue.start_seconds); update(); draw();
    }
    function render() {
        $("issues").replaceChildren(); $("markers").replaceChildren();
        $("measurements").hidden = !state.report;
        $("marker-label").hidden = !state.report;
        $("legend").hidden = !state.report;
        if (!state.report) return;
        const report = state.report;
        setText($("limits"), measured({ thresholds: report.thresholds, duration_seconds: report.duration_seconds,
            sample_rate: report.sample_rate, channels: report.channels, truncated: report.truncated }));
        $("notes").replaceChildren(...(report.notes || []).map(text => {
            const li = document.createElement("li"); li.textContent = text; return li;
        }));
        setText($("marker-label"), `Markers: ${state.snapshot.label}, saved revision ${state.snapshot.revision}. Markers locate scan times; they do not redraw the selected stem. Waveform lanes remain Before / Saved preview.`);
        for (const issue of report.issues) {
            const li = document.createElement("li"), button = document.createElement("button"),
                description = document.createElement("p"), details = document.createElement("details"),
                summary = document.createElement("summary"), evidence = document.createElement("pre");
            li.className = "doctor-issue";
            li.style.setProperty("--doctor-color", COLORS[issue.kind] || COLORS.possible_ending);
            button.dataset.doctorIssue = issue.id; button.setAttribute("aria-pressed", "false");
            button.textContent = `${issueTime(issue, report.sample_rate)} · ${issue.title} · ${issue.severity}`;
            button.onclick = () => select(issue.id);
            description.textContent = issue.description; summary.textContent = "Evidence / measured values";
            evidence.textContent = measured(issue.evidence); details.append(summary, evidence);
            li.append(button, description, details); $("issues").append(li);
            const marker = document.createElement("button");
            marker.type = "button"; marker.className = "doctor-marker";
            marker.dataset.doctorIssue = issue.id; marker.setAttribute("aria-pressed", "false");
            marker.setAttribute("aria-label", button.textContent); marker.title = button.textContent;
            marker.style.setProperty("--doctor-color", COLORS[issue.kind] || COLORS.possible_ending);
            marker.style.left = `${Math.max(0, Math.min(98, issue.start_seconds / report.duration_seconds * 100))}%`;
            marker.style.width = `${Math.max(0.5, Math.min(100, (issue.end_seconds - issue.start_seconds) / report.duration_seconds * 100))}%`;
            marker.onclick = () => select(issue.id); $("markers").append(marker);
        }
    }
    function update() {
        const current = getState(), input = target();
        const label = `${input.selectedOptions[0]?.textContent || input.value} · ${current.filename}`;
        setText($("target"), `Scan target: ${label}.${current.pendingMix ? " Live fader changes are pending: scans use the saved audio, not the live mix. Update mix preview before a fix." : ""}`);
        $("scan").disabled = current.busy || !current.info.loaded || state.scanning;
        $("download").disabled = !state.report || current.busy;
        $("fix").disabled = !state.canRepair({ ...current, target: input.value });
        $("strength").disabled = current.busy;
        for (const button of document.querySelectorAll("[data-doctor-issue]")) button.disabled = current.busy;
        const issue = state.issue;
        setText($("fix-help"), current.pendingMix ? "Update mix preview before repairing. A scan describes saved audio only."
            : issue?.repair ? `${issue.title}: preview a conservative ${issue.repair === "declip" ? "declip" : "low-mid reduction"} over ${issueTime(issue, state.report.sample_rate)}. Listen before applying.`
            : issue ? "This is a listening flag. No automatic repair is offered; silence and endings are never cut."
            : "Select an issue to listen. Only supported repairs offer a preview. No automatic fix-all or missing-audio reconstruction.");
    }
    $("scan").onclick = async () => {
        const current = getState();
        if (current.busy || state.scanning || !current.info.loaded) return;
        const input = target(), token = state.begin(input.value,
            `${input.selectedOptions[0].textContent} · ${current.filename}`);
        render(); update(); status(`Scanning ${token.label} (saved audio)…`); sync();
        try {
            const report = await client.call("doctor-scan", { target: token.target });
            if (state.accept(token, target().value, report)) {
                status(`${report.issues.length} listening flag${report.issues.length === 1 ? "" : "s"} for ${token.label}.${report.truncated ? " Report limit reached; some flags are omitted." : ""} Scanning changed no audio.`);
                render();
                document.getElementById("s-playback-details").open = true;
            }
        } catch (error) {
            if (state.current(token, target().value)) {
                state.scanning = false; status(`Scan failed: ${error.message || error}`);
            }
        } finally { update(); sync(); draw(); }
    };
    $("strength").oninput = () => setText($("strength-value"), `${Math.round(Number($("strength").value) * 100)}%`);
    $("fix").onclick = () => {
        // Guard the handler as well as the disabled control, including live mix changes.
        if (!state.canRepair({ ...getState(), target: target().value })) return;
        const issue = state.issue, strength = Number($("strength").value);
        if (!Number.isFinite(strength) || strength < 0 || strength > 1) return;
        const repair = { action: issue.repair, start_seconds: issue.start_seconds,
            end_seconds: issue.end_seconds, strength };
        const selectedTarget = state.snapshot.target;
        return job("Preparing a conservative Doctor preview…", () =>
            mutate("doctor", { target: selectedTarget, repair }), true, true);
    };
    $("download").onclick = () => {
        if (getState().busy) return;
        const value = state.download();
        if (value) download(JSON.stringify(value, null, 2), "Studio-issue-report.json", "application/json");
    };
    return {
        sync: update,
        get scanning() { return state.scanning; },
        invalidate(message = "Audio or target changed. Scan this saved target again.") {
            state.invalidate(); render(); status(message); update();
        },
        cancel() { state.invalidate(); render(); status("Scan cancelled. No audio or history changed."); update(); sync(); },
    };
}
