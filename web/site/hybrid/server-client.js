import { traceFetch } from "../session-trace.js";
import { serverEndpoint } from "./endpoint-policy.js";
import { capabilities, snapshot } from "./protocol.js";
import { ArtifactDownloadError, checkArtifactDownload, readArtifactDownload } from "./artifact-download.js";
import { readBoundedJson, readArrangementJson, JsonResponseError, AUTHENTICATED_JSON_LIMIT_BYTES, PROJECT_JSON_LIMIT_BYTES } from "./bounded-json.js";
import { readMasterWav } from "./pcm-transfer.js";
import { isRemoteMediaPath } from "./remote-media-policy.js";

export class ServerClient {
    #token = "";
    #requests = new Set();
    #epoch = 0;
    #fetch;
    constructor({ fetchImpl = globalThis.fetch.bind(globalThis), pageUrl = globalThis.location?.href } = {}) {
        this.#fetch = traceFetch(fetchImpl);
        this.pageUrl = pageUrl;
        this.endpoint = "";
    }
    async connect(endpoint, token) {
        this.disconnect();
        const epoch = this.#epoch;
        this.endpoint = serverEndpoint(endpoint, this.pageUrl);
        if (typeof token !== "string" || !token.trim() || /[\s\u0000-\u001f\u007f]/.test(token) || token.length > 4096)
            throw new Error("Enter a valid owner or invite token.");
        this.#token = token;
        try {
            const value = await this.#json("/v1/capabilities");
            try { return capabilities(value); }
            catch (error) { error.status = 200; throw error; }
        }
        catch (error) { if (epoch === this.#epoch) this.disconnect(); throw error; }
    }
    cancelPending() {
        for (const request of this.#requests) request.abort();
        this.#requests.clear();
    }
    disconnect() { ++this.#epoch; this.cancelPending(); this.#token = ""; }
    get connectionRevision() { return this.#epoch; }
    replyMediaAccess(port, path) {
        if (!isRemoteMediaPath(path))
            throw new Error("Invalid server media path.");
        port.postMessage({ path, endpoint: this.endpoint, token: this.#token });
    }
    remoteRequest(path, { method = "GET", body, binary = false, signal } = {}) {
        if (!/^\/v2\/(?:capabilities|projects(?:\/[A-Za-z0-9_/?=&.-]+)?)$/.test(path))
            throw new Error("Invalid server project path.");
        return this.#remoteRequest(path, { method, body, binary, signal },
            (response, signal) => readBoundedJson(response, { maxBytes: PROJECT_JSON_LIMIT_BYTES, signal }));
    }
    arrangementRequest(path, { method = "GET", body, signal, compact = false } = {}) {
        const base = /^\/v2\/projects\/[A-Za-z0-9_-]{1,128}\/arrangement$/;
        const jobs = /^\/v2\/projects\/[A-Za-z0-9_-]{1,128}\/arrangement\/jobs(?:\/[A-Za-z0-9_-]{1,128}(?:\/cancel)?)?$/;
        if ((!base.test(path) && !jobs.test(path)) || !["GET", "POST"].includes(method) ||
            (path.endsWith("/jobs") && method !== "POST") || (path.endsWith("/cancel") && method !== "POST"))
            throw new Error("Invalid server arrangement path.");
        return this.#remoteRequest(path, { method, body, signal }, compact
            ? (response, signal) => readBoundedJson(response, { signal })
            : (response, signal) => readArrangementJson(response, { signal }), 16 * 1024 * 1024);
    }
    #remoteRequest(path, { method, body, binary = false, signal }, read, bodyLimit = 0) {
        const options = { method, signal };
        if (body !== undefined) {
            options.body = binary ? body : JSON.stringify(body);
            if (bodyLimit && new TextEncoder().encode(options.body).byteLength > bodyLimit)
                throw new Error("Arrangement request exceeds the 16 MiB metadata budget.");
            options.headers = { "Content-Type": binary ? "application/octet-stream" : "application/json" };
        }
        return this.#request(path, options, read).catch(error => {
            if (signal?.aborted) throw signal.reason;
            this.onRemoteFailure?.(error); throw error;
        });
    }
    async #request(path, options, read, timeoutMs = 120000) {
        if (!this.#token) throw new Error("Connect with your owner or invite token first.");
        const controller = new AbortController();
        const external = options?.signal;
        const stop = () => controller.abort();
        external?.addEventListener("abort", stop, { once: true });
        if (external?.aborted) stop();
        this.#requests.add(controller);
        const timeout = setTimeout(() => controller.abort(), timeoutMs);
        try {
            const response = await this.#fetch(`${this.endpoint}${path}`, {
                ...options, headers: { ...options?.headers, Authorization: `Bearer ${this.#token}` },
                signal: controller.signal, credentials: "omit", cache: "no-store", redirect: "error", referrerPolicy: "no-referrer",
            });
            if (!response.ok) {
                if (response.status === 429) {
                    let value;
                    try { value = await readBoundedJson(response, { signal: controller.signal }); } catch { /* Keep the generic bounded failure below. */ }
                    const limits = {
                        maintenance_draining: "The server is preparing for maintenance and is temporarily not accepting new sessions or projects. Existing work is preserved. Please try again later.",
                        arrangement_capacity: "All server arrangement slots are occupied by retained sessions. Your local work is preserved. Wait for a slot, or save your work before deleting your own server project. Refreshing or switching Server Off does not release retained sessions.",
                        project_capacity: "The server has reached its retained-project capacity. Your local work is preserved. Wait for a slot, or save your work before deleting your own server project.",
                        guest_job_limit: "This guest session has used its processing allowance. Current work is kept in this tab, but the session cannot start another processing job.",
                        guest_job_ip_limit: "This connection has reached the server's hourly processing allowance. Current work is kept; retry after the reported limit resets.",
                        public_job_limit: "The public server has reached its hourly processing allowance. Current work is kept; retry after the reported limit resets.",
                        guest_admission_limit: "This guest session has used its track-admission allowance. Existing tracks and edits are kept.",
                    };
                    const code = value?.error === "Guest job rate limit reached" ? "guest_job_limit" : value?.error;
                    if (Object.hasOwn(limits, code)) {
                        const error = new Error(limits[code]);
                        error.status = 429; error.code = code; error.serverQuota = true;
                        const retry = response.headers.get("Retry-After");
                        if (/^\d{1,8}$/.test(retry ?? "")) {
                            error.retryAfterSeconds = Math.min(86400, Number(retry));
                            error.message += ` Retry in about ${error.retryAfterSeconds} seconds; availability is not guaranteed.`;
                        }
                        throw error;
                    }
                }
                const hint = response.status === 401 || response.status === 403 ? " Check your token." :
                    response.status === 404 ? " The job may have expired or been deleted." :
                    response.status === 409 ? " Refresh the job before retrying." :
                    response.status === 429 ? " The server is busy; retry later." : "";
                const error = new Error(`Server request failed (HTTP ${response.status}).${hint}`);
                error.status = response.status;
                throw error;
            }
            return await read(response, controller.signal);
        } catch (error) {
            if (controller.signal.aborted) throw new Error("Request stopped or timed out. Server work or deletion may not have stopped; check the job before retrying.");
            // Never reflect an exception/response body that could echo a token.
            if (error.serverQuota || error instanceof ArtifactDownloadError || error instanceof JsonResponseError || /^Server request failed \(HTTP \d+\)\./.test(error.message)) throw error;
            throw new Error("The server could not be reached or its response could not be read. Check network, server CORS and the site's connection policy, then retry.");
        } finally { clearTimeout(timeout); external?.removeEventListener("abort", stop); this.#requests.delete(controller); }
    }
    #json(path, method = "GET", body) {
        return this.#request(path, { method, ...(body === undefined ? {} : {
            headers: { "Content-Type": "application/json" }, body: JSON.stringify(body),
        }) }, (response, signal) => readBoundedJson(response, { maxBytes: AUTHENTICATED_JSON_LIMIT_BYTES, signal }));
    }
    async create(body) { return snapshot(await this.#json("/v1/jobs", "POST", body)); }
    async get(id) { return snapshot(await this.#json(this.#path(id)), id); }
    async upload(id, offset, body) {
        const value = await this.#request(`${this.#path(id)}/upload?offset=${offset}`, {
            method: "PUT", headers: { "Content-Type": "application/octet-stream" }, body,
        }, (response, signal) => readBoundedJson(response, { maxBytes: AUTHENTICATED_JSON_LIMIT_BYTES, signal }));
        return snapshot(value, id);
    }
    async run(id) { return snapshot(await this.#json(`${this.#path(id)}/run`, "POST", {}), id); }
    async delete(id) {
        if ((await this.#json(this.#path(id), "DELETE"))?.deleted !== true)
            throw new Error("Server deletion was not confirmed. Retry Delete server files.");
    }
    async artifact(id, artifact) {
        checkArtifactDownload(artifact);
        return this.#request(`${this.#path(id)}/artifacts/${encodeURIComponent(artifact.id)}`,
            { method: "GET" }, (response) => readArtifactDownload(response, artifact));
    }
    async masterAudio(id, artifact, dimensions) {
        return this.#request(`${this.#path(id)}/artifacts/${encodeURIComponent(artifact.id)}`,
            { method: "GET" }, (response) => readMasterWav(response, artifact, dimensions), 900000);
    }
    #path(id) { return `/v1/jobs/${encodeURIComponent(id)}`; }
}
