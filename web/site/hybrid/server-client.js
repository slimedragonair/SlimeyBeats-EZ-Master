import { serverEndpoint } from "./endpoint-policy.js";
import { capabilities, snapshot } from "./protocol.js";
import { ArtifactDownloadError, checkArtifactDownload, readArtifactDownload } from "./artifact-download.js";

export class ServerClient {
    #token = "";
    #requests = new Set();
    #epoch = 0;
    #fetch;
    constructor({ fetchImpl = globalThis.fetch.bind(globalThis), pageUrl = globalThis.location?.href } = {}) {
        this.#fetch = fetchImpl;
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
        try { return capabilities(await this.#json("/v1/capabilities")); }
        catch (error) { if (epoch === this.#epoch) this.disconnect(); throw error; }
    }
    cancelPending() {
        for (const request of this.#requests) request.abort();
        this.#requests.clear();
    }
    disconnect() { ++this.#epoch; this.cancelPending(); this.#token = ""; }
    async #request(path, options, read) {
        if (!this.#token) throw new Error("Connect with your owner or invite token first.");
        const controller = new AbortController();
        this.#requests.add(controller);
        const timeout = setTimeout(() => controller.abort(), 120000);
        try {
            const response = await this.#fetch(`${this.endpoint}${path}`, {
                ...options, headers: { ...options?.headers, Authorization: `Bearer ${this.#token}` },
                signal: controller.signal, credentials: "omit", cache: "no-store", redirect: "error", referrerPolicy: "no-referrer",
            });
            if (!response.ok) {
                const hint = response.status === 401 || response.status === 403 ? " Check your token." :
                    response.status === 404 ? " The job may have expired or been deleted." :
                    response.status === 409 ? " Refresh the job before retrying." :
                    response.status === 429 ? " The server is busy; retry later." : "";
                throw new Error(`Server request failed (HTTP ${response.status}).${hint}`);
            }
            return await read(response);
        } catch (error) {
            if (controller.signal.aborted) throw new Error("Request stopped or timed out. Server work or deletion may not have stopped; check the job before retrying.");
            // Never reflect an exception/response body that could echo a token.
            if (error instanceof ArtifactDownloadError || /^Server request failed \(HTTP \d+\)\./.test(error.message)) throw error;
            throw new Error("The server could not be reached or its response could not be read. Check network, server CORS and the site's connection policy, then retry.");
        } finally { clearTimeout(timeout); this.#requests.delete(controller); }
    }
    #json(path, method = "GET", body) {
        return this.#request(path, { method, ...(body === undefined ? {} : {
            headers: { "Content-Type": "application/json" }, body: JSON.stringify(body),
        }) }, (response) => response.json());
    }
    async create(body) { return snapshot(await this.#json("/v1/jobs", "POST", body)); }
    async get(id) { return snapshot(await this.#json(this.#path(id)), id); }
    async upload(id, offset, body) {
        const value = await this.#request(`${this.#path(id)}/upload?offset=${offset}`, {
            method: "PUT", headers: { "Content-Type": "application/octet-stream" }, body,
        }, (response) => response.json());
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
    #path(id) { return `/v1/jobs/${encodeURIComponent(id)}`; }
}
