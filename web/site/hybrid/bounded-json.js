export const PUBLIC_JSON_LIMIT_BYTES = 16 * 1024;
export const AUTHENTICATED_JSON_LIMIT_BYTES = 64 * 1024;
export const PROJECT_JSON_LIMIT_BYTES = 1024 * 1024;
export const ARRANGEMENT_JSON_LIMIT_BYTES = 33 * 1024 * 1024;

// Messages are authored here, never derived from an untrusted response body.
export class JsonResponseError extends Error {
    constructor(message) { super(message); this.status = 200; }
}

export async function readBoundedJson(response, { maxBytes = AUTHENTICATED_JSON_LIMIT_BYTES, signal } = {}) {
    if (!Number.isSafeInteger(maxBytes) || maxBytes < 1 || maxBytes > PROJECT_JSON_LIMIT_BYTES)
        throw new RangeError("Invalid JSON response budget.");
    return readJson(response, maxBytes, signal);
}

// Only the arrangement endpoint admits the larger document/catalog snapshot.
// Ordinary v1/v2 readers keep their existing limits.
export async function readArrangementJson(response, { maxBytes = ARRANGEMENT_JSON_LIMIT_BYTES, signal } = {}) {
    if (!Number.isSafeInteger(maxBytes) || maxBytes < 1 || maxBytes > ARRANGEMENT_JSON_LIMIT_BYTES)
        throw new RangeError("Invalid arrangement JSON response budget.");
    return readJson(response, maxBytes, signal, true);
}

async function readJson(response, maxBytes, signal, declaredAllocation = false) {
    let reader;
    const cancel = () => {
        // Cancellation must not hide the original failure or stall its deadline.
        try { (reader ? reader.cancel() : response.body?.cancel())?.catch(() => {}); }
        catch { /* Best effort: the bounded read still fails closed. */ }
    };
    const checkAbort = () => {
        if (signal?.aborted) throw new DOMException("JSON request stopped", "AbortError");
    };
    try {
        checkAbort();
        const type = response.headers?.get("Content-Type")?.split(";")[0].trim().toLowerCase();
        if (type !== "application/json" && !/^application\/[a-z0-9][a-z0-9!#$&^_.+-]*\+json$/.test(type || ""))
            throw new JsonResponseError("The server did not return a JSON Content-Type. Check the API setup.");
        const header = response.headers.get("Content-Length");
        if (header !== null && (!/^\d+$/.test(header.trim()) || !Number.isSafeInteger(Number(header)) || Number(header) > maxBytes))
            throw new JsonResponseError("Server JSON Content-Length is invalid or exceeds the response budget.");
        if (!response.body?.getReader)
            throw new JsonResponseError("Bounded JSON streaming is unavailable. Use a supported browser.");
        reader = response.body.getReader();
        signal?.addEventListener("abort", cancel, { once: true });
        checkAbort();
        // One fixed buffer bounds retained bytes and tiny-chunk object overhead.
        const bytes = new Uint8Array(declaredAllocation && header !== null ? Number(header) : maxBytes);
        let received = 0;
        while (true) {
            const { done, value } = await reader.read();
            checkAbort();
            if (done) break;
            if (!(value instanceof Uint8Array)) throw new JsonResponseError("The server returned an invalid JSON stream.");
            if (value.byteLength > maxBytes - received)
                throw new JsonResponseError("Server JSON exceeded the response budget; the stream was cancelled.");
            if (value.byteLength > bytes.byteLength - received)
                throw new JsonResponseError("Server JSON exceeded its declared Content-Length; the stream was cancelled.");
            bytes.set(value, received); received += value.byteLength;
        }
        try { return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes.subarray(0, received))); }
        catch { throw new JsonResponseError("The server returned malformed JSON. Check the API setup."); }
    } catch (error) {
        cancel(); throw error;
    } finally {
        signal?.removeEventListener("abort", cancel);
        reader?.releaseLock();
    }
}
