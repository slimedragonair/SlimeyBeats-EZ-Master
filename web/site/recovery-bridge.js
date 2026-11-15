import { checkBudget, validateSnapshot } from "./recovery-format.js";
let adapters;
const listeners = new Set();
export function connectRecovery(value) { adapters = value; changedRecovery(); }
export function changedRecovery() { for (const listener of listeners) listener(); }
export function subscribeRecovery(listener) { listeners.add(listener); }
export const workspaceRecovery = {
    busy: () => !adapters || adapters.master.busy || adapters.studio.busy,
    hasWork: () => !!adapters && (adapters.master.loaded || adapters.studio.loaded),
    context: () => adapters ? { engineReady: adapters.master.ready, masterLoaded: adapters.master.loaded,
        previewReady: adapters.master.preview, studioLoaded: adapters.studio.loaded,
        stems: adapters.studio.stemCount, busy: adapters.master.busy || adapters.studio.busy } : {},
    async capture() {
        if (!adapters || workspaceRecovery.busy()) throw new Error("Wait for the audio job before saving recovery.");
        checkBudget(adapters.master.estimate() + adapters.studio.estimate());
        const master = await adapters.master.capture(), studio = await adapters.studio.capture();
        return master || studio ? { master, studio } : null;
    },
    async prepare(value, current) {
        validateSnapshot(value);
        if (!adapters?.master.ready) throw new Error("The audio engine is unavailable. Keep the recovery file and retry after reloading; nothing was replaced.");
        let main, studio;
        try {
            if (value.master) main = await adapters.master.prepare(value.master, current);
            if (!current()) throw new Error("Newer work was kept; recovery was cancelled.");
            if (value.studio) studio = await adapters.studio.prepare(value.studio, current);
            return { commit() { main?.commit(); studio?.commit(); }, dispose() { main?.dispose(); studio?.dispose(); } };
        } catch (error) { main?.dispose(); studio?.dispose(); throw error; }
    },
};
