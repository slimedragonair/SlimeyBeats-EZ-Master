import { RecoveryStore, storageMessage } from "./recovery-store.js";
import { RECOVERY_VERSION, recoveryFile, readRecoveryFile } from "./recovery-format.js";
const idleMessage = "Autosave is off. New edits stay in tab memory. Previously saved recovery points remain until you delete them; manual recovery downloads are your choice.";
export function createRecovery({ capture, prepare, hasWork, busy, onChange = () => {}, store = new RecoveryStore() }) {
    const owner = crypto.randomUUID();
    let policy, fault = "", message = idleMessage, version = 0, sequence = 0, timer, saving = false, restoring = false, suspended = false;
    let releaseLock, disposed = false;
    // No expiry/heartbeat is treated as evidence that another tab has died.
    if (navigator.locks) navigator.locks.request(`slimeybeats-recovery:${owner}`, () =>
        disposed ? undefined : new Promise((resolve) => { releaseLock = resolve; })).catch(() => {});
    const channel = typeof BroadcastChannel === "function" ? new BroadcastChannel("slimeybeats-recovery-policy") : null;
    const emit = () => onChange();
    async function refresh() {
        try {
            const latest = await store.state();
            if (policy && policy.generation !== latest.generation) { ++version; clearTimeout(timer); fault = ""; }
            policy = latest;
            message = policy.enabled ? "Local recovery enabled. Saved audio can remain on this device after closing the tab." : idleMessage;
        }
        catch (e) { fault = storageMessage(e); }
        emit();
    }
    channel && (channel.onmessage = () => { ++version; clearTimeout(timer); refresh(); });
    const initial = refresh();
    function changed() {
        ++version; clearTimeout(timer);
        if (policy?.enabled && !fault && !suspended) timer = setTimeout(save, 1500);
    }
    async function snapshot() {
        const value = await capture();
        return value ? { version: RECOVERY_VERSION, ...value } : null;
    }
    async function save() {
        clearTimeout(timer);
        if (!policy?.enabled || fault || saving || restoring || suspended || busy()) return;
        const observed = version, generation = policy.generation;
        saving = true; message = "Saving a local recovery point…"; emit();
        try {
            const value = await snapshot();
            if (!value) message = "No loaded audio to save. Existing recovery points are kept.";
            if (value && observed === version && !busy()) {
                const saved = await store.save(owner, ++sequence, generation, value, () => observed === version && !busy());
                if (saved) message = "Recovery saved locally. Browser cleanup can still remove it; export work you want to keep.";
            }
        } catch (e) { fault = e.message?.startsWith("Recovery") ? e.message : storageMessage(e); }
        finally { saving = false; emit(); if (observed !== version && !fault && !suspended) timer = setTimeout(save, 1500); }
    }
    async function configure(enabled, remove = false) {
        ++version; clearTimeout(timer);
        policy = await store.configure(enabled, remove);
        fault = ""; message = enabled ? "Local audio recovery enabled on this device." : idleMessage;
        channel?.postMessage("policy-changed"); emit();
        if (enabled) changed();
    }
    async function restore(value, observed = version) {
        if (observed !== version) throw new Error("New work arrived while reading recovery. Your newer edits were kept.");
        if (restoring || busy()) throw new Error("Wait for the current job before restoring.");
        if (hasWork()) throw new Error("Current work is kept. Open this app in a new tab to restore, or export and close both workspaces first.");
        restoring = true; message = "Preparing recovery locally. Current work will be kept if anything changes or preparation fails."; emit();
        let staged;
        try {
            staged = await prepare(value, () => observed === version && !busy() && !hasWork());
            if (observed !== version || busy() || hasWork()) throw new Error("New work arrived while recovery was preparing. Your newer work was kept.");
            staged.commit(); // Both staged workspaces publish synchronously, with no async gap.
            ++version; message = "Recovered without playback. Review the result, then export when ready.";
        } finally { staged?.dispose(); restoring = false; emit(); }
        changed();
    }
    return {
        initial, changed, save, configure, refresh, owner,
        suspend() { suspended = true; ++version; clearTimeout(timer); },
        resume() { suspended = false; refresh(); if (hasWork()) changed(); },
        get state() { return { enabled: !!policy?.enabled, available: !!policy, saving, restoring, message: fault || message, paused: !!fault }; },
        async list() { await initial; return store.list(); },
        async restorePoint(id) {
            const observed = version;
            const pointOwner = id.split(":")[0];
            if (navigator.locks && pointOwner !== owner)
                return navigator.locks.request(`slimeybeats-recovery:${pointOwner}`, { ifAvailable: true }, async (lock) => {
                    if (!lock) throw new Error("That session is open in another tab. Close it before restoring. Its lock was not changed.");
                    return restore(await store.read(id), observed);
                });
            return restore(await store.read(id), observed);
        },
        async download() {
            const observed = version, value = await snapshot();
            if (observed !== version || busy()) throw new Error("The session changed while preparing its recovery download. Try again after the current edit finishes.");
            if (!value) throw new Error("Load audio before downloading a recovery copy.");
            return recoveryFile(value);
        },
        async importFile(file) { const observed = version; return restore(await readRecoveryFile(file), observed); },
        dispose() { disposed = true; clearTimeout(timer); ++version; releaseLock?.(); channel?.close(); },
    };
}
