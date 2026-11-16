import { ServerClient } from "./server-client.js";
import { CLIENT_CHUNK_BYTES, TERMINAL_STATES } from "./protocol.js";

const sourceKey = (file) => `${file.name}\n${file.size}\n${file.lastModified}`;

// This controller owns only the explicitly selected server file. It has no
// reference to the local engine, recovery adapters, preferences or storage.
export class ServerSession {
    #generation = 0;
    #timer;
    #wake;
    constructor({ client = new ServerClient(), onChange = () => {}, pollMs = 1500 } = {}) {
        this.client = client;
        this.onChange = onChange;
        this.pollMs = pollMs;
        this.connected = false;
        this.caps = null;
        this.file = null;
        this.consent = false;
        this.job = null;
        this.jobEndpoint = "";
        this.source = null;
        this.busy = "";
        this.message = "Local mastering is the default. Server studio is disconnected; no connection or upload has been requested.";
        this.error = false;
        this.creationUncertain = false;
    }
    get sameServer() { return this.connected && this.jobEndpoint === this.client.endpoint; }
    get canSubmit() {
        return this.connected && !this.busy && !!this.file && this.consent &&
            (!this.job || (this.sameServer && this.job.state === "uploading"));
    }
    #notify(message, error = false) {
        this.message = message; this.error = error; this.onChange(this);
    }
    #stop() {
        ++this.#generation;
        this.client.cancelPending();
        clearTimeout(this.#timer);
        this.#wake?.(); this.#wake = null;
        this.busy = "";
    }
    async #action(label, action) {
        if (this.busy) return;
        const generation = ++this.#generation;
        this.busy = label;
        this.#notify(label);
        const current = () => generation === this.#generation;
        try { await action(current); }
        catch (error) { if (current()) this.#notify(error.message, true); }
        finally { if (current()) { this.busy = ""; this.onChange(this); } }
    }
    async connect(endpoint, token) {
        if (this.connected || this.busy) return;
        await this.#action("Connecting to read server capabilities…", async (current) => {
            const caps = await this.client.connect(endpoint, token);
            if (!current()) return;
            this.caps = caps; this.connected = true;
            this.#notify(this.job
                ? "Connected. A previous job is remembered in this tab. Check its status or delete it at its original destination. Nothing was uploaded."
                : "Connected. Review the storage policy, then choose a separate WAV or MP3 for this server. No audio has been uploaded.");
        });
    }
    disconnect() {
        this.#stop(); this.client.disconnect();
        this.connected = false; this.caps = null; this.file = null; this.consent = false;
        this.#notify(this.job
            ? "Disconnected; token, selected file and consent cleared. Server work may continue. Reconnect to the original destination and choose Cancel & delete server files to request deletion, or wait for the displayed expiry. Deletion has not been confirmed."
            : this.creationUncertain
                ? "Disconnected; token, selected file and consent cleared. A job creation response was lost; an empty server job may remain until the server's retention deadline."
                : "Disconnected; token, selected file and consent cleared. Pending requests were stopped in this tab.");
    }
    select(file) {
        if (!this.connected || this.busy) return;
        this.file = null; this.consent = false;
        if (!file) { this.#notify("No server file selected."); return; }
        if (!/\.(wav|mp3)$/i.test(file.name) || !Number.isSafeInteger(file.size) || file.size <= 0)
            return this.#notify("Choose a nonempty WAV or MP3 file for the server preview.", true);
        if (file.size > this.caps.limits.max_upload_bytes)
            return this.#notify("This file exceeds the server's advertised upload limit. Nothing was uploaded.", true);
        if (this.job && (!this.sameServer || this.job.state !== "uploading" || sourceKey(file) !== this.source?.key))
            return this.#notify("Delete the existing server job before selecting a new source. To resume an upload, reselect the same unmodified file at the original destination.", true);
        this.file = file;
        this.#notify("Selected for server processing only. Review destination, size and retention, then give consent before uploading.");
    }
    consentToUpload(value) {
        if (this.busy) return;
        this.consent = !!value && this.connected && !!this.file;
        this.onChange(this);
    }
    #accept(job) {
        this.job = job;
        this.#notify(`${job.state}: ${job.message || "Server status received."}`);
    }
    async submit({ profile, format, mode }) {
        if (!this.canSubmit) return;
        if (!this.caps.profiles.some((p) => p.id === profile) || !this.caps.formats.includes(format) ||
            !["wav24", "mp3"].includes(format) || !["streaming_safe", "loud"].includes(mode)) {
            this.#notify("Choose a profile, format and mode offered by this server.", true); return;
        }
        await this.#action("Preparing the explicit server upload…", async (current) => {
            const file = this.file;
            if (!this.job) {
                // A lost POST response is ambiguous. Never retry it automatically.
                this.creationUncertain = true;
                let job;
                try {
                    job = await this.client.create({ filename: file.name, bytes: file.size, profile, format, mode,
                        target_lufs: null, ceiling_dbtp: -1, intensity: 1, consent: true });
                } catch (error) {
                    throw new Error(`${error.message} Job creation outcome is unknown. No file chunks were sent. Retrying may create another empty job; it will be subject to server retention.`);
                }
                if (!current()) return;
                this.creationUncertain = false;
                this.jobEndpoint = this.client.endpoint;
                this.source = { key: sourceKey(file), filename: file.name, bytes: file.size, profile, format, mode };
                this.#accept(job);
            } else {
                const job = await this.client.get(this.job.id);
                if (!current()) return;
                this.#accept(job);
            }
            if (this.job.total_bytes !== file.size) throw new Error("Server job size differs from the selected source. Delete this job before starting again.");
            while (this.job.state === "uploading" && this.job.uploaded_bytes < file.size) {
                const offset = this.job.uploaded_bytes;
                const end = Math.min(file.size, offset + this.caps.limits.chunk_bytes, offset + CLIENT_CHUNK_BYTES);
                const job = await this.client.upload(this.job.id, offset, file.slice(offset, end));
                if (!current()) return;
                this.#accept(job);
                if (job.total_bytes !== file.size || job.uploaded_bytes !== end)
                    throw new Error("The server did not acknowledge the expected upload offset. Check the job before retrying.");
            }
            if (this.job.state === "uploading") {
                const job = await this.client.run(this.job.id);
                if (!current()) return;
                this.#accept(job);
            }
            await this.#poll(current);
        });
    }
    async #poll(current) {
        while (current() && !TERMINAL_STATES.has(this.job.state) && this.job.state !== "uploading") {
            await new Promise((resolve) => { this.#wake = resolve; this.#timer = setTimeout(resolve, this.pollMs); });
            this.#wake = null;
            if (!current()) return;
            const job = await this.client.get(this.job.id);
            if (!current()) return;
            this.#accept(job);
        }
    }
    async refresh() {
        if (!this.sameServer || !this.job || this.busy) return;
        await this.#action("Checking server job…", async (current) => {
            const job = await this.client.get(this.job.id);
            if (!current()) return;
            this.#accept(job); await this.#poll(current);
        });
    }
    async delete() {
        if (!this.sameServer || !this.job || this.busy === "Deleting server files…") return;
        this.#stop();
        await this.#action("Deleting server files…", async (current) => {
            try { await this.client.delete(this.job.id); }
            catch (error) { throw new Error(`${error.message} Server deletion is NOT confirmed. Retry Cancel & delete server files; the job and its expiry are still shown.`); }
            if (!current()) return;
            this.job = null; this.source = null; this.jobEndpoint = ""; this.file = null; this.consent = false;
            this.#notify("Server confirmed cancellation and deletion of this job's files. Choose a file and consent again to start another job.");
        });
    }
    async download(artifactId, save) {
        if (!this.sameServer || this.busy || this.job?.state !== "completed") return;
        const artifact = this.job.artifacts.find((a) => a.id === artifactId);
        if (!artifact) return;
        await this.#action("Downloading server artifact…", async (current) => {
            const result = await this.client.artifact(this.job.id, artifact);
            if (!current()) return;
            save(result);
            this.#notify("Download prepared from an authenticated request. Server files remain until explicit deletion or the displayed expiry.");
        });
    }
}
