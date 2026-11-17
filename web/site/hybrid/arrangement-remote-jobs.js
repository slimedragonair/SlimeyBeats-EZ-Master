const ID = /^[A-Za-z0-9_-]{1,128}$/;
const STATES = new Set(["queued", "running", "completed", "failed", "cancelled"]);

function delay(signal) {
    return new Promise((resolve, reject) => {
        const finish = () => { signal.removeEventListener("abort", abort); resolve(); };
        const timer = setTimeout(finish, 350);
        const abort = () => { clearTimeout(timer); reject(signal.reason); };
        signal.addEventListener("abort", abort, { once: true });
        if (signal.aborted) abort();
    });
}

// Heavy work runs in an owned server job. Only metadata crosses this interface;
// artifacts stay on the authenticated browser download route.
export function createRemoteArrangementJobs({ request, getSnapshot, onError = () => {}, wait = delay }) {
    const pending = new Set();
    return {
        async run(operation, values, { expectedRevision, signal, onProgress = () => {} } = {}) {
            signal?.throwIfAborted();
            const base = getSnapshot();
            if (expectedRevision !== undefined && expectedRevision !== base.revision) throw new Error("The arrangement changed. Review this target before processing.");
            const controller = new AbortController(), abort = () => controller.abort(signal.reason);
            pending.add(controller); signal?.addEventListener("abort", abort, { once: true });
            if (signal?.aborted) abort();
            let jobId, terminal = false;
            const compact = !["open_project", "review_archive", "commit_import"].includes(operation);
            const validate = value => {
                if (!ID.test(value?.job_id || "") || (jobId && value.job_id !== jobId) || value.operation !== operation ||
                    value.epoch !== base.epoch || value.base_revision !== base.revision || !STATES.has(value.state) ||
                    !Number.isFinite(value.progress) || value.progress < 0 || value.progress > 1)
                    throw new Error("The server returned an invalid arrangement job. Current work was kept.");
                return value;
            };
            try {
                controller.signal.throwIfAborted();
                // Obtain an owned job ID even when cancellation arrives during
                // acknowledgement, so the server task can then be cancelled.
                let value = validate(await request("/jobs", { method: "POST", body: {
                    operation, epoch: base.epoch, expected_revision: base.revision, ...values,
                }, compact }));
                jobId = value.job_id;
                while (["queued", "running"].includes(value.state)) {
                    controller.signal.throwIfAborted();
                    onProgress(value.progress, `Server · ${operation.replaceAll("_", " ")} · ${Math.round(value.progress * 100)}%`);
                    await wait(controller.signal);
                    value = validate(await request(`/jobs/${jobId}`, { signal: controller.signal, compact }));
                }
                terminal = true; controller.signal.throwIfAborted();
                if (value.state !== "completed") throw new Error(value.state === "cancelled"
                    ? "Server processing was cancelled. Your arrangement was kept."
                    : "The server could not finish this arrangement job. Current work was kept.");
                const latest = getSnapshot();
                if (latest.epoch !== base.epoch || latest.revision !== base.revision)
                    throw new Error("The arrangement changed during processing. Its newer work was kept.");
                if (!value.result || typeof value.result !== "object") throw new Error("The arrangement job returned no result.");
                onProgress(1, "Server result ready."); return value.result;
            } finally {
                signal?.removeEventListener("abort", abort); pending.delete(controller);
                if (jobId && !terminal) {
                    try { await request(`/jobs/${jobId}/cancel`, { method: "POST", body: {}, compact: true }); }
                    catch { onError(new Error("Server cancellation could not be confirmed. Check the server job before retrying.")); }
                }
            }
        },
        clear() { for (const controller of pending) controller.abort(new DOMException("Arrangement processing stopped", "AbortError")); },
    };
}

export function checkedArrangementArtifact(value) {
    if (!ID.test(value?.id || "") || !Number.isSafeInteger(value.bytes) || value.bytes <= 0 ||
        typeof value.name !== "string" || !value.name || typeof value.content_type !== "string")
        throw new Error("The server returned an invalid arrangement download.");
    return value;
}
