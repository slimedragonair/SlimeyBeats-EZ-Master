// Private observability only: metadata and output taps, never substitute PCM.
(() => {
    const audit = window.arrangementRemoteAudit = { events: [], snapshots: [], streams: [], errors: [], fullMediaReads: 0 };
    const secrets = new Set(), owned = new Map(), original = window.fetch;
    const NativeAudio = window.Audio;
    audit.media = [];
    window.Audio = function (...args) { const element = new NativeAudio(...args); audit.media.push(element); return element; };
    window.Audio.prototype = NativeAudio.prototype;
    audit.mediaState = () => audit.media.map(element => ({ time: element.currentTime, duration: element.duration,
        paused: element.paused, ready: element.readyState, network: element.networkState,
        buffered: Array.from({ length: element.buffered.length }, (_, index) => [element.buffered.start(index), element.buffered.end(index)]) }));
    window.fetch = async function (input, options = {}) {
        const url = new URL(typeof input === "string" ? input : input.url, location.href);
        const headers = new Headers(options.headers || input?.headers);
        const authorization = headers.get("Authorization");
        if (authorization?.startsWith("Bearer ")) secrets.add(authorization.slice(7));
        const project = url.pathname.match(/^\/v2\/projects\/([A-Za-z0-9_-]+)(?:\/|$)/)?.[1];
        if (project && authorization) owned.set(project, authorization);
        let request;
        try { if (typeof options.body === "string") request = JSON.parse(options.body); } catch { /* Binary uploads are not inspected. */ }
        const response = await original.apply(this, arguments);
        if (/\/arrangement$/.test(url.pathname)) {
            const value = await response.clone().json();
            audit.events.push({ operation: request?.operation || "read", status: response.status, path: url.pathname, revision: value.revision,
                error_code: value.error_code || value.error });
            if (value.arrangement) { audit.latest = value; audit.snapshots.push(value); }
            if (value.stream_id) audit.streams.push(value);
        }
        return response;
    };
    const connect = AudioNode.prototype.connect;
    AudioNode.prototype.connect = function (target, ...rest) {
        const result = connect.call(this, target, ...rest);
        if (target === this.context.destination && !arrangementAudit.outputs.includes(this)) arrangementAudit.outputs.push(this);
        return result;
    };
    const read = Response.prototype.arrayBuffer;
    Response.prototype.arrayBuffer = function (...args) {
        if (/\/arrangement\/streams\//.test(this.url)) { ++audit.fullMediaReads; throw new Error("Arrangement previews must stay streamed"); }
        return read.apply(this, args);
    };
    audit.summary = () => {
        const storage = JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage }, cookie: document.cookie });
        const urls = [location.href, ...[...document.querySelectorAll("[src],[href]")].map(element => String(element.src || element.href || ""))];
        return { fullMediaReads: audit.fullMediaReads, credentials_memory_only: [...secrets].every(secret => !storage.includes(secret) && urls.every(url => !url.includes(secret))) };
    };
    audit.cleanup = async () => {
        const result = [];
        for (const [id, authorization] of owned) {
            const response = await original(`/v2/projects/${id}`, { method: "DELETE", headers: { Authorization: authorization } });
            result.push({ id, status: response.status });
        }
        return result;
    };
})();
