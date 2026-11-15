// Only disposable, freshly exported bytes may be transferred to this worker.
// Editable source/preview buffers remain with their existing DSP owners.
export function packageSession(workspace, payload, { signal } = {}) {
    if (signal?.aborted) return Promise.reject(new DOMException("Export cancelled", "AbortError"));
    if (!["mastering", "studio"].includes(workspace) || !payload || typeof payload !== "object" ||
        Array.isArray(payload) || (payload.files !== undefined && !Array.isArray(payload.files)))
        return Promise.reject(new Error("Invalid session download payload"));
    return new Promise((resolve, reject) => {
        const worker = new Worker(new URL("./session-export-worker.js", import.meta.url), { type: "module" });
        let settled = false;
        const finish = (error, blob) => {
            if (settled) return;
            settled = true;
            clearTimeout(timer);
            signal?.removeEventListener("abort", cancel);
            worker.terminate();
            error ? reject(error) : resolve(blob);
        };
        const cancel = () => finish(new DOMException("Export cancelled", "AbortError"));
        const timer = setTimeout(() => finish(new Error("Session packaging timed out. Try separate downloads.")), 120000);
        signal?.addEventListener("abort", cancel, { once: true });
        worker.onerror = (event) => finish(new Error(event.message || "Session packaging stopped"));
        worker.onmessageerror = () => finish(new Error("Session packaging returned unreadable data"));
        worker.onmessage = ({ data }) => data.error ? finish(new Error(data.error))
            : data.blob instanceof Blob ? finish(null, data.blob) : finish(new Error("Invalid session download"));
        try {
            const buffers = [payload.audio?.bytes, ...(payload.files || []).map((file) => file.bytes)]
                .filter((bytes) => bytes instanceof Uint8Array).map((bytes) => bytes.buffer);
            worker.postMessage({ workspace, payload }, [...new Set(buffers)]);
        }
        catch (error) { finish(error); }
    });
}
