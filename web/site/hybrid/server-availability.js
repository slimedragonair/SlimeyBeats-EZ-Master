import { PUBLIC_API_ORIGIN, serverEndpoint } from "./endpoint-policy.js";
import { publicRequest, PublicServerError } from "./public-request.js";

export const HEALTH_INTERVAL_MS = 30000;
export const AVAILABILITY_LABELS = Object.freeze({
    idle: "[Not checked]", checking: "[Checking server]", offline: "[Offline]",
    down: "[Server down]", disabled: "[Server disabled]", setup: "[Setup needed]",
    access: "[Access needed]", busy: "[Server busy]", online: "[Server online]",
});

export function failureStatus(error) {
    const status = error?.status ?? error?.cause?.status;
    if ([401, 403].includes(status)) return "access";
    if (status === 429) return "busy";
    return !status || (status >= 500 && status < 600) ? "down" : "setup";
}

export class ServerAvailability {
    #generation = 0;
    #request;
    #timer;
    constructor({ endpoint = PUBLIC_API_ORIGIN, pageUrl = globalThis.location?.href,
        probe = (origin, options) => publicRequest(origin, "/v1/status", { pageUrl, ...options }),
        onChange = () => {}, setTimer = (callback, ms) => setTimeout(callback, ms), clearTimer = (id) => clearTimeout(id),
        visible = true, online = true } = {}) {
        Object.assign(this, { endpoint, pageUrl, probe, onChange, setTimer, clearTimer, visible, online });
        this.enabled = false;
        this.state = "idle";
        this.detail = "Local processing is selected. No server check has been requested.";
        this.guestAccess = false;
        this.operatorAccess = false;
    }
    #publish(state, detail) { this.state = state; this.detail = detail; this.onChange(this); }
    #cancel() {
        ++this.#generation;
        this.#request?.abort(); this.#request = null;
        this.clearTimer(this.#timer); this.#timer = null;
    }
    setEnabled(value) {
        this.enabled = !!value;
        this.#cancel();
        if (!value && this.state === "checking") this.#publish("idle", "Server check paused. Local processing is selected.");
    }
    setEndpoint(value) {
        const endpoint = serverEndpoint(value, this.pageUrl);
        this.#cancel(); this.endpoint = endpoint; this.guestAccess = false;
        this.#publish("idle", "Check this destination before using it.");
    }
    setVisible(value) {
        this.visible = !!value;
        this.#cancel();
        if (this.visible && this.enabled) void this.check();
    }
    setOnline(value) {
        this.online = !!value;
        this.#cancel();
        if (!this.online) this.#publish("offline", "This device is offline. The server job is kept in this tab; retry when connected.");
        else if (this.enabled && this.visible) void this.check();
    }
    reportFailure(error) {
        this.#cancel();
        const state = failureStatus(error);
        const detail = state === "setup" ? "The server responded, but its API or connection setup is incompatible. Retry later or turn the switch off to use local processing."
            : state === "access" ? "The server responded, but access expired or was denied. Choose Retry to reconnect."
            : state === "busy" ? "The server is busy. Retry later; no audio is sent by reconnecting."
            : "Unable to reach the server. Check the connection and retry explicitly.";
        this.#publish(state, `${detail} Existing job references are kept in this tab.`);
        if (this.enabled && this.visible)
            this.#timer = this.setTimer(() => void this.check(), HEALTH_INTERVAL_MS);
    }
    async check() {
        this.#cancel();
        if (!this.visible) return;
        if (!this.online) { this.setOnline(false); return; }
        const generation = this.#generation;
        const request = this.#request = new AbortController();
        this.#publish("checking", "Checking availability only. No audio is sent.");
        try {
            const value = await this.probe(serverEndpoint(this.endpoint, this.pageUrl), { signal: request.signal });
            if (generation !== this.#generation) return;
            if (value?.api_version !== 1 || !["online", "busy", "disabled"].includes(value.status) ||
                typeof value.guest_access !== "boolean") throw new PublicServerError(200);
            this.guestAccess = value.guest_access;
            if (value.status === "busy") this.#publish("busy", "The server is busy. Retry later; no audio is sent by reconnecting.");
            else if (value.status === "disabled" && !this.operatorAccess) this.#publish("disabled", "The server is reachable, but public processing is disabled. No upload can start. Use local processing, or connect privately in advanced operator settings.");
            else this.#publish(value.guest_access || this.operatorAccess ? "online" : "access", value.guest_access || this.operatorAccess
                ? this.operatorAccess && !value.guest_access
                    ? "Private operator connection available. Choose Master to process the current track."
                    : "Server available. Use the normal Master button; the result loads into the editor."
                : "Guest access is unavailable. An operator can connect in advanced settings.");
        } catch (error) {
            if (generation === this.#generation) this.reportFailure(error);
        } finally {
            if (generation === this.#generation) {
                this.#request = null;
                if (this.enabled && this.visible)
                    this.#timer = this.setTimer(() => void this.check(), HEALTH_INTERVAL_MS);
            }
        }
    }
}
