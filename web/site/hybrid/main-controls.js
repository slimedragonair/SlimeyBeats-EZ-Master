// Processing selection is independent of the editor: both engines feed the
// same immutable-source / editable-master workflow.
const listeners = new Set();
let controller, runMaster, cancelMaster, locked = false;

export const serverProcessing = {
    get enabled() { return !!controller?.enabled; },
    attach(value, run, cancel) { controller = value; runMaster = run; cancelMaster = cancel; },
    subscribe(listener) { listeners.add(listener); },
    changed() { for (const listener of listeners) listener(); this.renderMainControls(); },
    renderMainControls(options) {
        if (options) locked = !!options.locked;
        if (typeof document === "undefined") return;
        // Never override the editor's file, settings, transport or export locks.
        const toggle = document.getElementById("hybrid-enabled");
        if (toggle) toggle.disabled = locked;
        const privacy = document.querySelector?.(".privacy-badge");
        if (privacy) privacy.textContent = this.enabled ? "Server mastering selected" : "Audio stays on this device";
        const engine = document.getElementById("engine-status");
        if (engine && document.documentElement?.dataset.engineState === "ready")
            engine.textContent = this.enabled ? "RUST · SERVER" : "RUST · LOCAL";
    },
    async master(audio, settings, reference, progress) {
        if (!this.enabled || !runMaster) throw new Error("Turn on server processing before mastering on the server.");
        return runMaster(audio, settings, reference, progress);
    },
    cancel() { cancelMaster?.(); },
};
