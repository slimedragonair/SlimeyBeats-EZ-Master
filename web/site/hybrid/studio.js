import { ServerSession } from "./server-session.js";
import { createStudioView } from "./studio-view.js";

function saveArtifact({ blob, name }) {
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url; link.download = name;
    document.body.append(link); link.click(); link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 30000);
}

export function wireServerStudio(host, options = {}) {
    const view = createStudioView(host);
    const session = new ServerSession({ ...options, onChange: (state) => view.render(state) });
    const control = view.byId;
    control("connect").addEventListener("click", () => {
        const token = control("token").value;
        control("token").value = "";
        void session.connect(control("endpoint").value, token);
    });
    function disconnect() {
        control("token").value = "";
        control("file").value = "";
        session.disconnect();
    }
    control("disconnect").addEventListener("click", disconnect);
    control("file").addEventListener("change", () => session.select(control("file").files[0]));
    control("consent").addEventListener("change", () => session.consentToUpload(control("consent").checked));
    control("submit").addEventListener("click", () => void session.submit({
        profile: control("profile").value, format: control("format").value, mode: control("mode").value,
    }));
    control("refresh").addEventListener("click", () => void session.refresh());
    control("delete").addEventListener("click", () => void session.delete());
    control("artifacts").addEventListener("click", (event) => {
        const button = event.target.closest("button[data-artifact-id]");
        if (button) void session.download(button.dataset.artifactId, options.saveArtifact || saveArtifact);
    });
    // Includes pagehide/BFCache: restored pages must reconnect and consent again.
    window.addEventListener("pagehide", disconnect);
    view.render(session);
    return session;
}

const host = typeof document === "undefined" ? null : document.getElementById("server-studio-host");
if (host) wireServerStudio(host);
