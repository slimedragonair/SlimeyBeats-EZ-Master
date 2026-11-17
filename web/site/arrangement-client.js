export function createArrangementClient() {
    const worker = new Worker(new URL("./arrangement-worker.js", import.meta.url), { type: "module" });
    const pending = new Map(); let sequence = 0, closed = false;
    const rejectAll = error => { for (const entry of pending.values()) entry.reject(error); pending.clear(); };
    worker.onmessage = ({ data }) => {
        const entry = pending.get(data.id);
        if (!entry) return;
        pending.delete(data.id);
        if (data.error) entry.reject(new Error(data.error)); else entry.resolve(data.value);
    };
    worker.onerror = event => { closed = true; rejectAll(new Error(event.message || "Arrangement worker failed")); worker.terminate(); };
    return {
        request(action, data = {}, { signal, transfer = [] } = {}) {
            if (closed) return Promise.reject(new Error("Arrangement worker is closed"));
            if (signal?.aborted) return Promise.reject(new DOMException("Cancelled", "AbortError"));
            // Once dispatched, metadata transactions finish atomically. Do not
            // pretend an accepted edit was rolled back by cancelling its reply.
            return new Promise((resolve, reject) => {
                const id = ++sequence; pending.set(id, { resolve, reject });
                try { worker.postMessage({ ...data, action, id }, transfer); }
                catch (error) { pending.delete(id); reject(error); }
            });
        },
        close() { closed = true; worker.terminate(); rejectAll(new Error("Arrangement worker closed")); },
    };
}
