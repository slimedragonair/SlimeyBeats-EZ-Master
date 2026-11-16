import { remoteMediaPath } from "./remote-media-policy.js";

// A Service Worker lets native <audio> and browser downloads use authenticated
// HTTP Range streams without keeping a full recording in JavaScript memory.
export class RemoteMediaBridge {
    constructor(client, { container = globalThis.navigator?.serviceWorker } = {}) {
        this.client = client;
        this.container = container;
        this.workerPath = new URL("../remote-media-worker.js", import.meta.url).pathname;
        this.allowed = new Set();
        this.downloadFrames = new Set();
        this.onMessage = event => {
            const value = event.data;
            if (event.source !== this.container?.controller || !this.ownedController() || !event.ports?.[0] ||
                value?.type !== "slimeybeats-media-access" || value.version !== 2) return;
            const port = event.ports[0];
            // Mastering and Studio share this worker. Leave an unowned request
            // untouched so the other workspace can answer on the same port.
            if (!this.allowed.has(value.path)) return;
            this.client.replyMediaAccess(port, value.path);
            port.close();
        };
        container?.addEventListener("message", this.onMessage);
    }
    async ready() {
        if (!this.container) throw new Error("Server streaming needs Service Workers. Use a current browser with site storage enabled.");
        if (!this.starting) this.starting = this.#start().catch(error => { this.starting = null; throw error; });
        return this.starting;
    }
    ownedController() {
        try { return new URL(this.container?.controller?.scriptURL).pathname === this.workerPath; }
        catch { return false; }
    }
    async #start() {
        const scope = new URL("../", import.meta.url);
        const worker = new URL("../remote-media-worker.js", import.meta.url);
        await this.container.register(worker, { scope: scope.pathname });
        await this.container.ready;
        if (!this.ownedController()) await new Promise((resolve, reject) => {
            const changed = () => { if (this.ownedController()) { clearTimeout(timer); this.container.removeEventListener("controllerchange", changed); resolve(); } };
            const timer = setTimeout(() => { this.container.removeEventListener("controllerchange", changed); reject(new Error("Server streaming could not start. Reload this page and retry.")); }, 10000);
            this.container.addEventListener("controllerchange", changed);
            changed();
        });
    }
    url(project, id, kind = "preview") {
        const path = remoteMediaPath(project, id, kind);
        this.allowed.add(path);
        return new URL(`../remote-media/${project}/${kind}/${id}`, import.meta.url).href;
    }
    async download(project, artifact, name) {
        await this.ready();
        const url = this.url(project, artifact.id, "artifact");
        await new Promise((resolve, reject) => {
            const channel = new MessageChannel();
            const timer = setTimeout(() => { channel.port1.close(); reject(new Error("Download could not connect to this editing tab. Retry Export.")); }, 5000);
            channel.port1.onmessage = ({ data }) => {
                clearTimeout(timer); channel.port1.close();
                data?.ready ? resolve() : reject(new Error("Download access is unavailable. Retry Export."));
            };
            this.container.controller.postMessage({ type: "slimeybeats-download-route", url }, [channel.port2]);
        });
        // Chromium may bypass Service Workers for a[download]. A navigation
        // to the attachment response keeps authentication in the worker and
        // lets the browser stream straight to its download destination.
        const frame = document.createElement("iframe");
        frame.hidden = true; frame.title = name || artifact.name || "Audio export";
        frame.onload = () => { this.downloadFrames.delete(frame); frame.remove(); };
        this.downloadFrames.add(frame);
        frame.src = url; document.body.append(frame);
    }
    clear() { this.allowed.clear(); for (const frame of this.downloadFrames) frame.remove(); this.downloadFrames.clear(); }
    dispose() { this.clear(); this.container?.removeEventListener("message", this.onMessage); }
}
