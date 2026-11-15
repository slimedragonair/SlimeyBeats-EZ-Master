import { validateSnapshot, snapshotSize } from "./recovery-format.js";
import { encodeRecoveryPayload, decodeRecoveryPayload } from "./recovery-payload.js";
import { retentionPlan } from "./recovery-retention.js";
export const RECOVERY_DB = "slimeybeats.opt-in-recovery";
const request = (r) => new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error);
});
const finished = (tx) => new Promise((resolve, reject) => {
    tx.oncomplete = resolve; tx.onabort = tx.onerror = (event) =>
        reject(tx.error || event.target?.error || new Error("Recovery storage transaction failed."));
});
export function storageMessage(error) {
    return error?.name === "QuotaExceededError"
        ? "Recovery is paused: site storage is full. Previous restore points remain. Download a recovery copy or export audio; editing still works."
        : "Recovery storage is unavailable or blocked. Your audio remains in this tab. Download a recovery copy or export audio before leaving.";
}
export class RecoveryStore {
    constructor(factory) { this.factory = factory; }
    async open() {
        if (this.db) return this.db;
        const factory = this.factory || globalThis.indexedDB;
        if (!factory) throw new Error("Site storage unavailable");
        if (!this.opening) this.opening = new Promise((resolve, reject) => {
            const r = factory.open(RECOVERY_DB, 2);
            r.onupgradeneeded = () => {
                const db = r.result;
                if (!db.objectStoreNames.contains("config")) db.createObjectStore("config");
                if (!db.objectStoreNames.contains("points")) db.createObjectStore("points", { keyPath: "id" });
                const payloads = db.createObjectStore("payloads", { keyPath: "id" });
                // Upgrade is atomic. Earlier completed points remain readable;
                // no asynchronous Blob conversion is attempted inside this tx.
                const cursor = r.transaction.objectStore("points").openCursor();
                cursor.onsuccess = () => {
                    const next = cursor.result;
                    if (!next) return;
                    const { snapshot, ...metadata } = next.value;
                    if (snapshot) { payloads.put({ id: metadata.id, snapshot }); next.update(metadata); }
                    next.continue();
                };
            };
            r.onblocked = () => reject(new Error("Another tab is blocking recovery storage. Close it and retry."));
            r.onerror = () => reject(r.error);
            r.onsuccess = () => {
                const opened = r.result;
                this.db = opened;
                opened.onversionchange = () => {
                    opened.close();
                    if (this.db === opened) { this.db = null; this.opening = null; }
                };
                resolve(this.db);
            };
        });
        try { return await this.opening; }
        catch (error) { this.opening = null; throw error; }
    }
    async state() {
        const db = await this.open(), tx = db.transaction("config");
        const value = await request(tx.objectStore("config").get("policy"));
        if (value && (value.version !== 1 || typeof value.enabled !== "boolean" || !Number.isSafeInteger(value.generation)))
            throw new Error("Unrecognized recovery preferences. Saved audio was not changed.");
        return value || { version: 1, enabled: false, generation: 0 };
    }
    async configure(enabled, remove = false) {
        const db = await this.open(), tx = db.transaction(["config", "points", "payloads"], "readwrite"), done = finished(tx);
        done.catch(() => {});
        const store = tx.objectStore("config"), current = await request(store.get("policy"));
        const oldGeneration = Number.isSafeInteger(current?.generation) && current.generation >= 0 && current.generation < Number.MAX_SAFE_INTEGER
            ? current.generation : Date.now();
        const policy = { version: 1, enabled, generation: oldGeneration + 1 };
        store.put(policy, "policy");
        if (remove) { tx.objectStore("points").clear(); tx.objectStore("payloads").clear(); }
        await done;
        return policy;
    }
    async list() {
        const db = await this.open(), tx = db.transaction("points");
        // The index contains no audio: list/getAll never clones stored PCM.
        const rows = await request(tx.objectStore("points").getAll());
        return rows.sort((a, b) => b.saved - a.saved || b.sequence - a.sequence).map(({ snapshot, ...meta }) => meta);
    }
    async read(id) {
        const db = await this.open(), tx = db.transaction(["points", "payloads"]);
        const point = await request(tx.objectStore("points").get(id));
        if (!point) throw new Error("This recovery point is no longer available.");
        if (!Number.isSafeInteger(point.bytes) || point.bytes > 128 * 1048576 || point.bytes < 0)
            throw new Error("Invalid saved recovery size. Current work was kept.");
        const payload = await request(tx.objectStore("payloads").get(id));
        if (!payload) throw new Error("Recovery audio is incomplete. Current work was kept.");
        const snapshot = decodeRecoveryPayload(payload.snapshot);
        validateSnapshot(snapshot);
        return snapshot;
    }
    async save(owner, sequence, generation, snapshot, current = () => true) {
        validateSnapshot(snapshot);
        const bytes = snapshotSize(snapshot), encoded = await encodeRecoveryPayload(snapshot), db = await this.open();
        if (!current()) return false;
        const tx = db.transaction(["config", "points", "payloads"], "readwrite"), done = finished(tx);
        // Attach a rejection handler before awaiting requests so an abort never leaks.
        done.catch(() => {});
        const points = tx.objectStore("points");
        const policy = await request(tx.objectStore("config").get("policy"));
        const rows = await request(points.getAll());
        if (!current() || !policy?.enabled || generation !== policy.generation) { await done; return false; }
        const owned = rows.filter((r) => r.owner === owner).sort((a, b) => b.sequence - a.sequence);
        if (owned.some((r) => r.sequence >= sequence)) { await done; return false; }
        let remove;
        try { remove = retentionPlan(rows, owner, bytes); }
        catch (error) { await done; throw error; }
        try {
            const payloads = tx.objectStore("payloads"), id = `${owner}:${sequence}`;
            for (const old of remove) { points.delete(old); payloads.delete(old); }
            const label = String(snapshot.master?.name || snapshot.studio?.filename || "Audio session").slice(0, 160);
            const kind = snapshot.master && snapshot.studio ? "Mastering + Studio" : snapshot.studio ? "Studio" : "Mastering";
            points.put({ id, owner, sequence, saved: Date.now(), bytes, label, kind });
            payloads.put({ id, snapshot: encoded });
        } catch (error) {
            tx.abort(); await done.catch(() => {}); throw error;
        }
        await done;
        return true;
    }
}
