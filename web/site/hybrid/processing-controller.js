import { ServerAvailability } from "./server-availability.js";
import { guestSession } from "./public-request.js";

// Mode changes never discard a guest's only job credential. Only explicit
// advanced Disconnect (or leaving the page) clears the authenticated session.
export class ProcessingController {
    #generation = 0;
    #access;
    constructor({ session, availability, requestGuest = guestSession, onChange = () => {},
        pageUrl = globalThis.location?.href } = {}) {
        Object.assign(this, { session, requestGuest, onChange, pageUrl });
        this.enabled = false;
        this.mainFile = null;
        this.connecting = false;
        this.availability = availability || new ServerAvailability({ pageUrl });
        this.availability.onChange = () => {
            const state = this.availability.state;
            const stopped = ["offline", "down", "access", "disabled", "setup"].includes(state);
            if (stopped && this.connecting) {
                ++this.#generation; this.#access?.abort(); this.connecting = false;
            }
            if (state !== "checking") session.setAvailable(state === "online", stopped);
            this.onChange(this);
        };
        session.setProcessingEnabled(false);
    }
    async setEnabled(value) {
        this.enabled = !!value;
        ++this.#generation; this.#access?.abort(); this.connecting = false;
        this.availability.setEnabled(this.enabled);
        this.session.setAvailable(false);
        this.session.setProcessingEnabled(this.enabled);
        this.onChange(this);
        if (this.enabled) {
            const generation = this.#generation;
            await this.check();
            if (generation === this.#generation && this.enabled && this.session.connected && this.mainFile && !this.session.job)
                this.session.select(this.mainFile);
        }
    }
    selectMainFile(file) {
        this.mainFile = file || null;
        if (this.enabled && this.session.connected) this.session.select(file);
        this.onChange(this);
    }
    async check() {
        const generation = this.#generation;
        await this.availability.check();
        if (!this.enabled || generation !== this.#generation || this.session.connected ||
            this.session.busy || this.connecting || this.session.job ||
            this.availability.state !== "online" || !this.availability.guestAccess) return;
        const access = this.#access = new AbortController();
        this.connecting = true; this.onChange(this);
        try {
            const guest = await this.requestGuest(this.availability.endpoint, { signal: access.signal, pageUrl: this.pageUrl });
            if (generation !== this.#generation || this.availability.state !== "online") return;
            await this.session.connect(this.availability.endpoint, guest.token);
            if (generation !== this.#generation) return;
            if (this.session.connected && this.mainFile) this.session.select(this.mainFile);
        } catch (error) {
            if (generation === this.#generation) this.availability.reportFailure(error);
        } finally {
            if (generation === this.#generation) { this.connecting = false; this.onChange(this); }
        }
    }
    async connectOperator(endpoint, token) {
        ++this.#generation; this.#access?.abort(); this.connecting = false;
        this.availability.setEndpoint(endpoint);
        await this.session.connect(this.availability.endpoint, token);
        this.availability.operatorAccess = this.session.connected;
        if (this.session.connected) {
            if (this.mainFile) this.session.select(this.mainFile);
            await this.availability.check();
        }
    }
    disconnect() {
        ++this.#generation; this.#access?.abort(); this.connecting = false;
        this.availability.operatorAccess = false;
        this.session.disconnect(); this.onChange(this);
    }
    forgetUnavailableJob() {
        if (!this.session.canForgetUnavailableJob) return false;
        ++this.#generation; this.#access?.abort(); this.connecting = false;
        this.availability.operatorAccess = false;
        // Cancel stale health and require a separate explicit connection intent.
        this.availability.setEndpoint(this.availability.endpoint);
        const forgotten = this.session.forgetUnavailableJob();
        this.onChange(this);
        return forgotten;
    }
}
