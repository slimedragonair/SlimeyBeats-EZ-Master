import { ServerSession } from "./server-session.js";
import { createStudioView } from "./studio-view.js";
import { ProcessingController } from "./processing-controller.js";
import { ServerAvailability } from "./server-availability.js";
import { serverProcessing } from "./main-controls.js";
import { runServerMaster } from "./master-runner.js";

export function wireServerStudio(host, options = {}) {
    const view = createStudioView(host);
    let controller, reportedFailure, active;
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
    const cancel = () => {
        active?.abort(new DOMException("Server mastering cancelled", "AbortError"));
        session.cancel();
    };
    serverProcessing.attach(controller, async (audio, settings, reference, progress) => {
        if (active) throw new Error("Wait for the current server master.");
        const operation = active = new AbortController();
        try {
            if (session.canForgetUnavailableJob) controller.forgetUnavailableJob();
            else if (!session.job && availability.state === "access") controller.disconnect();
            if (!session.connected || !session.available) await controller.check();
            operation.signal.throwIfAborted();
            return await runServerMaster(session, audio, settings, reference, { signal: operation.signal, progress });
        } finally { if (active === operation) active = null; render(); }
    }, cancel);
    control("enabled").addEventListener("change", () => {
        if (!control("enabled").checked) cancel();
        void controller.setEnabled(control("enabled").checked);
    });
    control("check").addEventListener("click", async () => {
        if (session.canForgetUnavailableJob) controller.forgetUnavailableJob();
        else if (!session.job && availability.state === "access") controller.disconnect();
        await controller.check();
    });
    control("delete").addEventListener("click", () => void session.delete());
    document.addEventListener("visibilitychange", () => availability.setVisible(!document.hidden));
    window.addEventListener("offline", () => availability.setOnline(false));
    window.addEventListener("online", () => availability.setOnline(true));
    window.addEventListener("pagehide", () => {
        cancel(); void controller.setEnabled(false); controller.disconnect();
    });
    render();
    return session;
}

const host = typeof document === "undefined" ? null : document.getElementById("server-studio-host");
if (host) wireServerStudio(host);
