import { ServerSession } from "./server-session.js";
import { createStudioView } from "./studio-view.js";
import { ProcessingController } from "./processing-controller.js";
import { ServerAvailability } from "./server-availability.js";
import { serverProcessing } from "./main-controls.js";

function saveArtifact({ blob, name }) {
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url; link.download = name;
    document.body.append(link); link.click(); link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 30000);
}

export function wireServerStudio(host, options = {}) {
    const view = createStudioView(host);
    let controller, reportedFailure;
    function render() {
        if (session.lastFailure && session.lastFailure !== reportedFailure && controller) {
            reportedFailure = session.lastFailure;
            controller.availability.reportFailure(reportedFailure);
        }
        view.render(session, controller);
        serverProcessing.changed();
    }
    const session = new ServerSession({ ...options, onChange: render });
    const availability = new ServerAvailability({ ...options, visible: !document.hidden, online: navigator.onLine });
    controller = new ProcessingController({ ...options, session, availability, onChange: render });
    const control = view.byId;
    serverProcessing.attach(controller, () => {
        const target = session.unavailableJobStatus ? control("forget")
            : !session.connected ? control("check") : !session.file ? control("file") : control("consent");
        (session.unavailableJobStatus ? control("unavailable-job")
            : session.file ? control("destination").closest("section") : control("panel")).scrollIntoView({ block: "start" });
        if (!target.disabled) target.focus();
    });
    control("enabled").addEventListener("change", () => void controller.setEnabled(control("enabled").checked));
    control("check").addEventListener("click", () => void controller.check());
    control("connect").addEventListener("click", async () => {
        const token = control("token").value;
        control("token").value = "";
        try {
            await controller.connectOperator(control("endpoint").value, token);
        } catch {
            availability.reportFailure({ status: 403 });
        }
    });
    function disconnect() {
        control("token").value = "";
        control("file").value = "";
        controller.disconnect();
    }
    control("disconnect").addEventListener("click", disconnect);
    control("file").addEventListener("change", () => serverProcessing.select(control("file").files[0]));
    control("consent").addEventListener("change", () => session.consentToUpload(control("consent").checked));
    control("submit").addEventListener("click", () => void session.submit({
        profile: control("profile").value, format: control("format").value, mode: control("mode").value,
    }));
    control("refresh").addEventListener("click", () => void session.refresh());
    control("delete").addEventListener("click", () => void session.delete());
    control("forget").addEventListener("click", () => {
        if (controller.forgetUnavailableJob()) {
            control("token").value = "";
            control("check").focus();
        }
    });
    control("artifacts").addEventListener("click", (event) => {
        const button = event.target.closest("button[data-artifact-id]");
        if (button) void session.download(button.dataset.artifactId, options.saveArtifact || saveArtifact);
    });
    // Capture these routes independently of local-engine startup and decoding.
    document.getElementById("file").addEventListener("change", (event) => {
        if (!controller.enabled) return;
        event.stopImmediatePropagation();
        serverProcessing.select(event.target.files[0]); event.target.value = "";
    }, true);
    document.getElementById("add").addEventListener("click", (event) => {
        if (!controller.enabled) return;
        event.stopImmediatePropagation(); document.getElementById("file").click();
    }, true);
    for (const id of ["master", "mobile-master"]) document.getElementById(id).addEventListener("click", (event) => {
        if (!controller.enabled) return;
        event.stopImmediatePropagation(); serverProcessing.review();
    }, true);
    document.addEventListener("drop", (event) => {
        if (!controller.enabled || document.body.classList.contains("studio-mode")) return;
        event.preventDefault(); event.stopImmediatePropagation();
        document.body.classList.remove("dragging");
        serverProcessing.select(event.dataTransfer.files[0]);
    }, true);
    document.addEventListener("visibilitychange", () => availability.setVisible(!document.hidden));
    window.addEventListener("offline", () => availability.setOnline(false));
    window.addEventListener("online", () => availability.setOnline(true));
    // Includes BFCache: mode returns off and must be explicitly opted in again.
    window.addEventListener("pagehide", () => { void controller.setEnabled(false); disconnect(); });
    render();
    return session;
}

const host = typeof document === "undefined" ? null : document.getElementById("server-studio-host");
if (host) wireServerStudio(host);
