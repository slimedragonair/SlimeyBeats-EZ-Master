import { serverEndpoint } from "./endpoint-policy.js";
import { readBoundedJson, JsonResponseError, PUBLIC_JSON_LIMIT_BYTES } from "./bounded-json.js";

export const PUBLIC_REQUEST_TIMEOUT_MS = 5000;

export class PublicServerError extends Error {
    constructor(status = 0) {
        super("Unable to reach the server. Check the connection and retry.");
        this.status = status;
    }
}

// Bound both fetch and JSON consumption, including transports that ignore abort.
// These two public routes never receive credentials, filenames or audio.
export async function publicRequest(endpoint, path, { pageUrl = globalThis.location?.href,
    fetchImpl = globalThis.fetch.bind(globalThis), signal, method = "GET",
    setTimer = setTimeout, clearTimer = clearTimeout } = {}) {
    const origin = serverEndpoint(endpoint, pageUrl);
    if (!["/v1/status", "/v1/session"].includes(path)) throw new Error("Unknown public server route.");
    const controller = new AbortController();
    let timer, rejectStopped;
    const stopped = new Promise((_, reject) => { rejectStopped = reject; });
    const stop = () => { controller.abort(); rejectStopped(new PublicServerError()); };
    signal?.addEventListener("abort", stop, { once: true });
    timer = setTimer(stop, PUBLIC_REQUEST_TIMEOUT_MS);
    try {
        if (signal?.aborted) stop();
        return await Promise.race([stopped, (async () => {
            const response = await fetchImpl(`${origin}${path}`, {
                method, signal: controller.signal, credentials: "omit", cache: "no-store",
                redirect: "error", referrerPolicy: "no-referrer",
                ...(method === "POST" ? { headers: { "Content-Type": "application/json" }, body: "{}" } : {}),
            });
            if (!response.ok) throw new PublicServerError(response.status);
            return await readBoundedJson(response, { maxBytes: PUBLIC_JSON_LIMIT_BYTES, signal: controller.signal });
        })()]);
    } catch (error) {
        // Never reflect a response or transport error that could contain secrets.
        throw error instanceof PublicServerError || error instanceof JsonResponseError ? error : new PublicServerError();
    } finally {
        clearTimer(timer);
        signal?.removeEventListener("abort", stop);
    }
}

export async function guestSession(endpoint, options) {
    const value = await publicRequest(endpoint, "/v1/session", { ...options, method: "POST" });
    if (typeof value?.token !== "string" || !value.token || value.token.length > 4096 ||
        /[\s\u0000-\u001f\u007f]/.test(value.token) ||
        typeof value.expires_at !== "string" || !Number.isFinite(Date.parse(value.expires_at)))
        throw new PublicServerError(200);
    return value;
}
