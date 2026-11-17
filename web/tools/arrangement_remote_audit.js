// Private observability only: metadata and output taps, never substitute PCM.
(() => {
    const audit = window.arrangementRemoteAudit = { events: [], mediaEvents: [], snapshots: [], streams: [], uploads: [], jobs: [], errors: [], fullMediaReads: 0, largeBlobReads: 0, clientDecodes: 0, serverPhase: false };
    for (const context of new Set([window.AudioContext, window.webkitAudioContext].filter(Boolean))) {
        const decode = context.prototype.decodeAudioData;
        context.prototype.decodeAudioData = function (...args) {
            if (audit.serverPhase) { ++audit.clientDecodes; throw new Error("Server arrangement assets must not decode on this device"); }
            return decode.apply(this, args);
        };
    }
    const secrets = new Set(), owned = new Map(), original = window.fetch;
    const cancellations = []; let deliberateStop;
    document.addEventListener("click", event => {
        const control = event.target.closest?.("#s-before,#s-after,#s-stop,#s-play");
        if (control && (control.id !== "s-play" || control.textContent === "Pause"))
            deliberateStop = { control: control.id, time: Date.now() };
    }, true);
    const NativeAudio = window.Audio;
    audit.media = [];
    window.Audio = function (...args) {
        const element = new NativeAudio(...args), remove = element.removeAttribute.bind(element);
        const record = event => audit.mediaEvents.push({ event, at: Date.now(), time: element.currentTime, ready: element.readyState, paused: element.paused });
        for (const event of ['loadedmetadata', 'play', 'playing', 'pause', 'canplay', 'waiting', 'ended', 'seeking', 'seeked', 'error']) element.addEventListener(event, () => record(event));
        const play = element.play.bind(element);
        element.play = () => { record('play-called'); const result = play(); result?.then(() => record('play-resolved'), () => record('play-rejected')); return result; };
        element.removeAttribute = name => {
            if (name === "src" && /\/remote-media\/[^/]+\/arrangement\//.test(element.getAttribute("src") || "") &&
                deliberateStop && Date.now() - deliberateStop.time < 250)
                cancellations.push({ control: deliberateStop.control, time: Date.now() });
            return remove(name);
        };
        audit.media.push(element); return element;
    };
    window.Audio.prototype = NativeAudio.prototype;
    audit.mediaState = () => audit.media.map(element => ({ time: element.currentTime, duration: Number.isFinite(element.duration) ? element.duration : null,
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
        if (url.pathname === "/v2/capabilities") audit.capabilities = await response.clone().json();
        if (/\/arrangement\/jobs$/.test(url.pathname) && response.status === 202) audit.jobs.push({ operation: request.operation });
        if (/\/uploads$/.test(url.pathname) && request?.role) {
            const value = await response.clone().json(); audit.uploads.push({ id: value.id, bytes: request.bytes, role: request.role });
        }
        if (/\/arrangement$/.test(url.pathname)) {
            const value = await response.clone().json();
            audit.events.push({ operation: request?.operation || "read", status: response.status, path: url.pathname, revision: value.revision,
                at: Date.now(),
                epoch: request?.epoch, expected_revision: request?.expected_revision, stream_id: request?.stream_id,
                error_code: value.error_code || value.error });
            if (value.arrangement) { audit.latest = value; audit.snapshots.push(value); }
            if (value.stream_id) audit.streams.push(value);
        }
        return response;
    };
    const blobRead = Blob.prototype.arrayBuffer;
    Blob.prototype.arrayBuffer = function (...args) {
        if (this.size > 128 * 1024 ** 2) { ++audit.largeBlobReads; throw new Error("Large archives must use chunked upload, not whole-file browser reads"); }
        return blobRead.apply(this, args);
    };
    const connect = AudioNode.prototype.connect;
    AudioNode.prototype.connect = function (target, ...rest) {
        const result = connect.call(this, target, ...rest);
        if (target === this.context.destination && !arrangementAudit.outputs.includes(this)) arrangementAudit.outputs.push(this);
        return result;
    };
    const read = Response.prototype.arrayBuffer;
    Response.prototype.arrayBuffer = function (...args) {
        if (/\/(?:arrangement\/streams|artifacts)\//.test(this.url)) { ++audit.fullMediaReads; throw new Error("Arrangement audio and downloads must stay streamed"); }
        return read.apply(this, args);
    };
    audit.summary = () => {
        const storage = JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage }, cookie: document.cookie });
        const urls = [location.href, ...[...document.querySelectorAll("[src],[href]")].map(element => String(element.src || element.href || ""))];
        return { fullMediaReads: audit.fullMediaReads, largeBlobReads: audit.largeBlobReads, clientDecodes: audit.clientDecodes, credentials_memory_only: [...secrets].every(secret => !storage.includes(secret) && urls.every(url => !url.includes(secret))) };
    };
    audit.cleanup = async () => {
        audit.cleanupAt = Date.now();
        const result = [];
        for (const [id, authorization] of owned) {
            const response = await original(`/v2/projects/${id}`, { method: "DELETE", headers: { Authorization: authorization } });
            result.push({ id, status: response.status });
        }
        return result;
    };
    audit.consumeCancellation = () => {
        const value = cancellations.find(value => !value.observed && Date.now() - value.time < 2000);
        if (!value) return null;
        value.observed = true; return { control: value.control, elapsed_ms: Date.now() - value.time };
    };
})();
