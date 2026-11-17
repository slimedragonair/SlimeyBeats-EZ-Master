import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./arrangement_public_audit.js', import.meta.url), 'utf8');
const api = 'https://slimeybeats-preview.tailf8de2d.ts.net';
const guest = { Authorization: 'Bearer ordinary-guest' };
function fixture(limit = 16) {
    const calls = [], reserved = []; let reports;
    class FakeElement extends EventTarget { removeAttribute(name) { this.removed = name; } }
    class FakeMedia extends FakeElement {
        constructor() { super(); Object.assign(this, { src: 'https://slimedragonair.github.io/SlimeyBeats-EZ-Master/current-work/remote-media/owned-project/arrangement/stream', currentTime: .144, duration: .144, ended: true, paused: true, readyState: 2 }); }
        play(...args) { this.playArguments = args; return this.playResult = Promise.resolve(); }
        pause() { ++this.pauseCalls; }
    }
    const context = vm.createContext({ URL, Headers, Date, Promise, setTimeout,
        Element: FakeElement, HTMLMediaElement: FakeMedia, Audio: FakeMedia,
        location: { href: 'https://slimedragonair.github.io/SlimeyBeats-EZ-Master/current-work/' },
        document: { querySelector: () => null },
        arrangementPublicConfig: { api, expectedHeavy: 6 },
        arrangementRemoteAudit: { pendingRequests: 0, media: [] },
        reservePublicJob: operation => { reserved.push(operation); if (reserved.length > 14) throw new Error('budget'); },
        reportPublicAudit: value => { reports = value; },
        fetch: async (url, options = {}) => {
            calls.push({ url, method: options.method || 'GET' });
            const path = new URL(url).pathname;
            const value = path === '/v1/session' ? { token: 'ordinary-guest' }
                : path === '/v2/capabilities' ? { limits: { guest_jobs_per_session: limit, guest_jobs_per_ip_hour: 32, guest_jobs_per_hour: 128 } }
                    : path === '/v2/projects' ? { id: 'owned-project' } : {};
            return new Response(JSON.stringify(value), { status: ['/v1/session', '/v2/projects'].includes(path) ? 201 : 200 });
        } });
    context.window = context; vm.runInContext(source, context);
    return { context, calls, reserved, reports: () => reports,
        async create() {
            context.arrangementPublicAudit.unlock();
            await context.fetch(api + '/v1/session', { method: 'POST' });
            await context.fetch(api + '/v2/capabilities', { headers: guest });
            return context.fetch(api + '/v2/projects', { method: 'POST', headers: guest });
        } };
}

test('publication mismatch cannot create a public guest or project', async () => {
    const f = fixture();
    await assert.rejects(f.context.fetch(api + '/v1/session', { method: 'POST' }), /approved publication/);
    assert.equal(f.calls.length, 0);
});

test('cleanup deletes only a witnessed newly created project at its real HTTPS origin', async () => {
    const f = fixture(); await f.create();
    await assert.rejects(f.context.fetch(api + '/v2/projects/another-users-project', { method: 'DELETE', headers: guest }), /not created/);
    await f.context.arrangementRemoteAudit.cleanup();
    assert.deepEqual(f.calls.filter(value => value.method === 'DELETE'), [{ url: api + '/v2/projects/owned-project', method: 'DELETE' }]);
    assert.equal(f.reports().cleanup[0].id, 'owned-project');
});

test('owner credentials and cross-origin credential forwarding are refused', async () => {
    const f = fixture(); await f.create();
    await assert.rejects(f.context.fetch(api + '/v2/projects/owned-project', { method: 'DELETE', headers: { Authorization: 'Bearer owner' } }), /ordinary guest/);
    await assert.rejects(f.context.fetch('https://elsewhere.invalid/', { headers: guest }), /different origin/);
});

test('advertised operator quotas are preserved before project creation', async () => {
    const f = fixture(3);
    await assert.rejects(f.create(), /limits do not admit/);
    assert.equal(f.calls.some(value => new URL(value.url).pathname === '/v2/projects'), false);
});

test('processing reservation happens before HTTP and cannot dispatch an untyped operation', async () => {
    const f = fixture(); await f.create();
    const path = api + '/v2/projects/owned-project/arrangement/jobs';
    for (let i = 0; i < 14; ++i) await f.context.fetch(path, { method: 'POST', headers: guest, body: '{"operation":"export"}' });
    await assert.rejects(f.context.fetch(path, { method: 'POST', headers: guest, body: '{"operation":"export"}' }), /budget/);
    await assert.rejects(f.context.fetch(path, { method: 'POST', headers: guest, body: '{"operation":"arbitrary_tool"}' }), /Unexpected/);
    assert.equal(f.calls.filter(value => value.url === path).length, 14);
});

test('media tracing preserves native Play promises and event delivery', async () => {
    const f = fixture(), audio = new f.context.Audio(); let ended = 0;
    audio.addEventListener('ended', () => ++ended);
    const result = audio.play('sentinel');
    assert.equal(result, audio.playResult); assert.deepEqual(audio.playArguments, ['sentinel']);
    audio.dispatchEvent(new Event('ended')); audio.removeAttribute('src');
    assert.equal(ended, 1); assert.equal(audio.removed, 'src');
    const trace = f.context.arrangementPublicAudit.stats.transport_trace;
    assert.equal(trace[0].event, 'ended'); assert.equal(trace[1].event, 'src-removed');
    assert.equal(trace[1].time, .144); assert.equal(trace[1].ended, true);
});
