export const COMPRESSED_PREVIEW_MIME = 'audio/mp4; codecs="mp4a.6B"';
const AHEAD_SECONDS = 1;
const KEEP_SECONDS = 2;
const START_SECONDS = .5;

function wait(target, event, signal) {
    return new Promise((resolve, reject) => {
        const clean = () => { target.removeEventListener(event, done); target.removeEventListener("error", failed); signal.removeEventListener("abort", aborted); };
        const done = () => { clean(); resolve(); };
        const failed = () => { clean(); reject(new Error("The compressed server preview could not be buffered.")); };
        const aborted = () => { clean(); reject(signal.reason); };
        target.addEventListener(event, done, { once: true }); target.addEventListener("error", failed, { once: true });
        signal.addEventListener("abort", aborted, { once: true }); if (signal.aborted) aborted();
    });
}

function tick(signal) {
    return new Promise((resolve, reject) => {
        const done = () => { signal.removeEventListener("abort", aborted); resolve(); };
        const timer = setTimeout(done, 50);
        const aborted = () => { clearTimeout(timer); reject(signal.reason); };
        signal.addEventListener("abort", aborted, { once: true }); if (signal.aborted) aborted();
    });
}

export function supportsCompressedPreview() {
    return !!globalThis.MediaSource?.isTypeSupported(COMPRESSED_PREVIEW_MIME);
}

// Append only current compressed bytes. A continuous HTTP response without a
// length makes some native media loaders wait for EOF; MSE keeps it playable
// while retaining a short window instead of the recording's full duration.
export function createCompressedPreview(audio, address, duration, onError) {
    if (!Number.isFinite(duration) || duration <= 0) throw new Error("The prepared server preview has no valid duration.");
    const media = new MediaSource(), abort = new AbortController(), signal = abort.signal;
    const opened = wait(media, "sourceopen", signal), url = URL.createObjectURL(media);
    let reader, disposed = false, resolveReady, rejectReady;
    const ready = new Promise((resolve, reject) => { resolveReady = resolve; rejectReady = reject; });
    void ready.catch(() => {});
    const closed = () => { if (!disposed) abort.abort(new Error("The server preview was detached unexpectedly.")); };
    media.addEventListener("sourceclose", closed);
    const timeout = setTimeout(() => abort.abort(new Error("The server preview did not open. Retry playback.")), 30000);
    async function pump() {
        await opened; clearTimeout(timeout); signal.throwIfAborted();
        const buffer = media.addSourceBuffer(COMPRESSED_PREVIEW_MIME); buffer.mode = "segments"; media.duration = duration;
        const response = await fetch(address, { signal, credentials: "omit", cache: "no-store", redirect: "error" });
        if (!response.ok || !/^audio\/mp4(?:;|$)/i.test(response.headers.get("Content-Type") || "") || !response.body) {
            void response.body?.cancel().catch(() => {});
            throw new Error("The server preview stream is unavailable. Reconnect and retry playback.");
        }
        reader = response.body.getReader();
        const update = async action => {
            signal.throwIfAborted();
            const done = wait(buffer, "updateend", signal);
            try { action(); } catch (error) { abort.abort(error); await done.catch(() => {}); throw error; }
            await done;
        };
        const append = async bytes => {
            while (buffer.buffered.length && buffer.buffered.end(buffer.buffered.length - 1) - audio.currentTime > AHEAD_SECONDS) await tick(signal);
            const cutoff = audio.currentTime - KEEP_SECONDS;
            if (buffer.buffered.length && cutoff > buffer.buffered.start(0) + .5) await update(() => buffer.remove(0, cutoff));
            await update(() => buffer.appendBuffer(bytes));
            if (buffer.buffered.length && buffer.buffered.end(buffer.buffered.length - 1) - buffer.buffered.start(0) >= START_SECONDS) resolveReady();
        };
        while (!signal.aborted) {
            const next = await reader.read();
            if (next.done) break;
            await append(next.value);
        }
        signal.throwIfAborted();
        const end = buffer.buffered.length ? buffer.buffered.end(buffer.buffered.length - 1) : 0;
        if (end + .05 < duration) throw new Error("The server preview ended before its prepared duration.");
        resolveReady();
        if (media.readyState === "open") media.endOfStream();
    }
    const task = pump().catch(error => {
        abort.abort(error); void reader?.cancel().catch(() => {});
        rejectReady(error);
        if (!disposed) onError(error);
    }).finally(() => { clearTimeout(timeout); try { reader?.releaseLock(); } catch { /* Pending cancellation releases the reader below. */ } });
    return { url, ready, dispose() {
        if (disposed) return;
        disposed = true; clearTimeout(timeout); abort.abort(new DOMException("Server preview stopped", "AbortError")); rejectReady(signal.reason);
        media.removeEventListener("sourceclose", closed);
        void reader?.cancel().catch(() => {}); URL.revokeObjectURL(url); void task;
    } };
}
