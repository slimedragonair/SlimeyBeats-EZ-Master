export class StudioClient {
    constructor() {
        this.sequence = 0;
        this.pending = new Map();
    }
    start() {
        if (this.worker) return;
        this.worker = new Worker(
            new URL("./studio-worker.js", import.meta.url),
            { type: "module" },
        );
        this.worker.onmessage = ({ data }) => {
            const job = this.pending.get(data.id);
            if (!job) return;
            this.pending.delete(data.id);
            data.error
                ? job.reject(new Error(data.error))
                : job.resolve(data.result);
        };
        this.worker.onerror = (event) =>
            this.stop(new Error(event.message || "Studio worker stopped"));
    }
    call(action, values = {}, transfer = []) {
        this.start();
        const id = ++this.sequence;
        return new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject });
            this.worker.postMessage({ id, action, ...values }, transfer);
        });
    }
    stop(
        error = new DOMException(
            "Studio processing cancelled; reload the source to continue",
            "AbortError",
        ),
    ) {
        this.worker?.terminate();
        this.worker = null;
        for (const job of this.pending.values()) job.reject(error);
        this.pending.clear();
    }
}
