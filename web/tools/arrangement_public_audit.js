// Public acceptance guard. Native HTTPS requests/responses pass through unchanged.
(() => {
    const config = window.arrangementPublicConfig, native = window.fetch.bind(window);
    const guests = new Set(), created = new Map();
    const jobs = new Set(['open_project', 'save_project', 'suggest_eq', 'export', 'review_archive', 'commit_import']);
    const stats = { sessions: 0, created: [], cleanup: [], jobs: [], violations: [], capabilities: null, media_sources: [], bridge_paths: [], transport_trace: [] };
    let unlocked = false;
    const refuse = message => { stats.violations.push(message); throw new Error(message); };
    if (window.HTMLMediaElement) {
        const record = (element, event, stack = false) => stats.transport_trace.push({
            event, at: Date.now(), time: element.currentTime, duration: Number.isFinite(element.duration) ? element.duration : null,
            ended: element.ended, paused: element.paused, ready: element.readyState,
            ...(stack ? { stack: new Error().stack } : {}) });
        const play = HTMLMediaElement.prototype.play;
        HTMLMediaElement.prototype.play = function (...args) {
            const url = new URL(this.currentSrc || this.src, location.href);
            stats.media_sources.push({ protocol: url.protocol, origin: url.origin, path: url.pathname, query: !!url.search });
            return play.apply(this, args);
        };
        const pause = HTMLMediaElement.prototype.pause;
        HTMLMediaElement.prototype.pause = function (...args) { record(this, 'pause-called', true); return pause.apply(this, args); };
        const remove = Element.prototype.removeAttribute;
        Element.prototype.removeAttribute = function (name) {
            if (this instanceof HTMLMediaElement && name === 'src') record(this, 'src-removed', true);
            return remove.apply(this, arguments);
        };
        const Audio = window.Audio;
        window.Audio = function (...args) {
            const element = new Audio(...args);
            for (const event of ['playing', 'waiting', 'pause', 'ended', 'durationchange'])
                element.addEventListener(event, () => record(element, event), true);
            return element;
        };
        window.Audio.prototype = Audio.prototype;
    }
    async function cleanup() {
        const audit = window.arrangementRemoteAudit;
        document.querySelector('#s-stop')?.click();
        const deadline = Date.now() + 5000; let quiet = Date.now();
        while (Date.now() < deadline) {
            await new Promise(resolve => setTimeout(resolve, 25));
            if (audit.pendingRequests || audit.media.some(value => !value.paused)) quiet = Date.now();
            else if (Date.now() - quiet >= 100) break;
        }
        if (audit.pendingRequests || audit.media.some(value => !value.paused)) refuse('Owned preview did not stop before cleanup');
        for (const [id, authorization] of created) {
            if (!guests.has(authorization)) refuse('Cleanup requires a witnessed ordinary guest');
            const response = await native(config.api + '/v2/projects/' + id,
                { method: 'DELETE', headers: { Authorization: authorization } });
            stats.cleanup.push({ id, status: response.status });
            if (![200, 204, 404].includes(response.status)) refuse('Owned project cleanup failed');
        }
        stats.service_worker = window.navigator?.serviceWorker?.controller?.scriptURL || null;
        await window.reportPublicAudit(stats);
        return stats.cleanup;
    }
    window.arrangementPublicAudit = { stats, unlock() {
        if (!window.arrangementRemoteAudit) refuse('Remote audit was not initialized');
        unlocked = true; window.arrangementRemoteAudit.cleanup = cleanup;
    } };
    window.fetch = async function (input, options = {}) {
        const url = new URL(typeof input === 'string' ? input : input.url, location.href);
        const method = String(options.method || input?.method || 'GET').toUpperCase();
        const authorization = new Headers(options.headers || input?.headers).get('Authorization');
        const write = !['GET', 'HEAD', 'OPTIONS'].includes(method);
        if (authorization && url.origin !== config.api) refuse('Credentials targeted a different origin');
        if (write && (url.origin !== config.api || !unlocked)) refuse('Public writes require the approved publication');
        if (url.origin !== config.api) {
            if (url.pathname.includes('/remote-media/')) stats.bridge_paths.push(url.pathname);
            return native(...arguments);
        }
        if (authorization && !guests.has(authorization)) refuse('Only a witnessed ordinary guest is allowed');
        const project = url.pathname.match(/^\/v2\/projects\/([A-Za-z0-9_-]{1,128})(?:\/|$)/)?.[1];
        if (project && (!created.has(project) || created.get(project) !== authorization)) refuse('Project was not created by this acceptance context');
        if (write && url.pathname === '/v2/projects') {
            const limits = stats.capabilities?.limits;
            if (!limits || ['guest_jobs_per_session', 'guest_jobs_per_ip_hour', 'guest_jobs_per_hour'].some(
                key => !Number.isSafeInteger(limits[key]) || limits[key] < config.expectedHeavy))
                refuse('Advertised guest limits do not admit the planned workflow');
        }
        if (write && /\/arrangement\/jobs$/.test(url.pathname)) {
            let body; try { body = JSON.parse(options.body); } catch { refuse('Expected a typed arrangement job'); }
            if (!jobs.has(body.operation)) refuse('Unexpected public processing operation');
            await window.reservePublicJob(body.operation);
            stats.jobs.push({ operation: body.operation });
        }
        const response = await native(...arguments);
        if (url.pathname === '/v1/session' && method === 'POST' && response.status === 201) {
            if (authorization) refuse('Guest creation unexpectedly supplied authorization');
            const value = await response.clone().json();
            if (typeof value.token !== 'string' || !value.token) refuse('Guest session did not return a token');
            guests.add('Bearer ' + value.token); ++stats.sessions;
        }
        if (url.pathname === '/v2/capabilities' && response.ok) stats.capabilities = await response.clone().json();
        if (url.pathname === '/v2/projects' && method === 'POST' && response.status === 201) {
            const value = await response.clone().json();
            if (!/^[A-Za-z0-9_-]{1,128}$/.test(value.id || '') || !guests.has(authorization)) refuse('Project creation ownership was not proven');
            created.set(value.id, authorization); stats.created.push(value.id);
        }
        return response;
    };
})();
