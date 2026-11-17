import { renderEngineGuidance } from "./engine-guidance.js";

// Processing selection is independent of the editor: both engines feed the
// same immutable-source / editable-master workflow.
const listeners = new Set();
let controller, runMaster, cancelMaster, locked = false;
let localLimits, remoteSource = false;

export const serverProcessing = {
    get enabled() { return !!controller?.enabled; },
    get preparing() {
        return this.enabled && !controller.session?.connected &&
            (!!controller.connecting || controller.availability?.state === "checking");
    },
    attach(value, run, cancel) { controller = value; runMaster = run; cancelMaster = cancel; },
    subscribe(listener) { listeners.add(listener); },
    changed() { for (const listener of listeners) listener(); this.renderMainControls(); },
    renderMainControls(options) {
        if (options) {
            if (Object.hasOwn(options, "locked")) locked = !!options.locked;
            if (Object.hasOwn(options, "localLimits")) localLimits = options.localLimits;
            if (Object.hasOwn(options, "remoteSource")) remoteSource = options.remoteSource;
        }
        if (typeof document === "undefined") return;
        renderEngineGuidance({ server: this.enabled, remoteSource, localLimits });
        // Never override the editor's file, settings, transport or export locks.
        const toggle = document.getElementById("hybrid-enabled");
        if (toggle) toggle.disabled = locked;
        const privacy = document.querySelector?.(".privacy-badge");
        if (privacy) privacy.textContent = this.enabled ? "Server processing selected" : "Local processing selected";
        const engine = document.getElementById("engine-status");
        if (engine && document.documentElement?.dataset.engineState === "ready")
            engine.textContent = this.enabled ? "RUST · SERVER" : "RUST · LOCAL";
    },
    async master(audio, settings, reference, progress) {
        if (!this.enabled || !runMaster) throw new Error("Turn on server processing before mastering on the server.");
        return runMaster(audio, settings, reference, progress);
    },
    cancel() { cancelMaster?.(); },
    async connection() {
        if (!this.enabled || !controller) throw new Error("Turn on Server processing to use the server tools.");
        const { session, availability } = controller;
        if (session.canForgetUnavailableJob) controller.forgetUnavailableJob();
        else if (!session.job && availability.state === "access") controller.disconnect();
        if (!session.connected || !session.available) await controller.check();
        if (controller.connecting) {
            const deadline = Date.now() + 120000;
            while (controller.connecting && this.enabled && Date.now() < deadline)
                await new Promise(resolve => setTimeout(resolve, 50));
        }
        if (!session.connected || !session.available || !this.enabled)
            throw new Error("The server is unavailable. Retry the connection or choose local processing.");
        return session.client;
    },
    failed(error) {
        if (error?.name === "AbortError" || ["guest_job_limit", "guest_admission_limit"].includes(error?.code)) return;
        const status = error?.status;
        if (!status || status === 200 || [401, 403, 429].includes(status) || status >= 500)
            controller?.availability?.reportFailure(error);
    },
};
