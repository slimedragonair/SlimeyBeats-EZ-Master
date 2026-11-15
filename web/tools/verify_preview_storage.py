"""Exercise same-origin production/preview storage and compact preview identity."""
import argparse
import http.server
import json
from pathlib import Path
import re
import threading
from urllib.parse import unquote, urlparse

from playwright.sync_api import sync_playwright, expect
from verify_studio_browser import fixture

ROOT = Path(__file__).resolve().parents[2]
IDENTITY = {
    'branch': 'current-work', 'commit': '1' * 40,
    'built_at': '2026-11-15T12:00:00Z',
    'run_url': 'https://github.com/slimedragonair/SlimeyBeats-EZ-Master/actions/runs/123',
    'main_commit': '2' * 40,
}


class Server(http.server.ThreadingHTTPServer):
    request_queue_size = 64


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def translate_path(self, path):
        relative = unquote(urlparse(path).path).lstrip('/')
        site = self.server.main_site
        if relative.startswith('current-work/'):
            site, relative = self.server.preview_site, relative[len('current-work/'):]
        target = (site / relative).resolve()
        return str(target if target.is_relative_to(site) else site / '__invalid__')

    def do_GET(self):
        path = urlparse(self.path).path
        if path.endswith('/__preview_probe__.html'):
            body, content_type = b'<!doctype html><title>Local storage probe</title>', 'text/html'
        elif path == '/current-work/preview-build.json':
            body, content_type = json.dumps(IDENTITY).encode(), 'application/json'
        else:
            return super().do_GET()
        self.send_response(200)
        self.send_header('Content-Type', content_type)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def probe(context, base, prefix):
    page = context.new_page()
    page.goto(base + prefix + '__preview_probe__.html')
    return page, page.evaluate('''async () => {
        const {createUiPreferences, UI_PREFERENCES_KEY, UI_TERMINAL_KEY} = await import('./ui-preferences.js');
        const {RecoveryStore, RECOVERY_DB} = await import('./recovery-store.js');
        const {createRecovery} = await import('./recovery-controller.js');
        const store = new RecoveryStore(), state = store.state.bind(store);
        window.probe = {prefs:createUiPreferences(), store, refreshes:0, events:[]};
        store.state = async () => { ++probe.refreshes; return state(); };
        probe.prefs.subscribe(source => probe.events.push(source));
        probe.controller = createRecovery({store, capture:async()=>null,
            prepare:async()=>({commit:()=>{probe.restored=true},dispose:()=>{}}),
            hasWork:()=>false, busy:()=>false});
        await probe.controller.initial;
        return {prefs:UI_PREFERENCES_KEY, terminal:UI_TERMINAL_KEY, db:RECOVERY_DB,
            state:await store.state(), choices:probe.prefs.state, owner:probe.controller.owner};
    }''')


def refreshes(page):
    return page.evaluate('probe.refreshes')


def storage_checks(context, base, checks):
    root, original = probe(context, base, '')
    preview, isolated = probe(context, base, 'current-work/')
    root_peer, _ = probe(context, base, '')
    preview_peer, _ = probe(context, base, 'current-work/')
    expected = {'prefs': 'slimeybeats.interface-preferences',
                'terminal': 'slimeybeats.interface-preferences.tutorial-dismissed',
                'db': 'slimeybeats.opt-in-recovery'}
    for name, value in expected.items():
        assert original[name] == value, original
        assert isolated[name] == 'current-work:' + value, isolated
    assert not original['state']['enabled'] and not isolated['state']['enabled']
    root.evaluate("probe.prefs.setAdvanced(true); probe.prefs.tutorialChoice('completed')")
    root_peer.wait_for_function("probe.prefs.state.tutorial === 'completed'")
    assert preview.evaluate('probe.prefs.state.tutorial') == 'unseen'
    preview.evaluate("probe.prefs.tutorialChoice('declined')")
    preview_peer.wait_for_function("probe.prefs.state.tutorial === 'declined'")
    assert root.evaluate('probe.prefs.state.tutorial') == 'completed'
    assert not preview.evaluate('probe.prefs.state.advanced')
    for page in (root, preview):
        page.reload()
        module = './ui-preferences.js'
        state = page.evaluate('async module => (await import(module)).createUiPreferences().state', module)
        assert state['tutorial'] == ('completed' if page == root else 'declined'), state
    # Recreate controllers after reload; the browser has released the old locks.
    root.close(); preview.close()
    root, original = probe(context, base, '')
    preview, isolated = probe(context, base, 'current-work/')
    checks.append('root defaults unchanged; preferences, terminal markers and storage events isolated across reload')

    before_root, before_preview = refreshes(root_peer), refreshes(preview_peer)
    root.evaluate('probe.controller.configure(true)')
    root_peer.wait_for_function('(before) => probe.refreshes > before', arg=before_root)
    assert refreshes(preview_peer) == before_preview
    assert not preview.evaluate('(async()=> (await probe.store.state()).enabled)()')
    root.evaluate('''async () => {
        const policy = await probe.store.state();
        probe.point = {version:1, master:{name:'local-root-fixture',
            audio:new Blob([new Float32Array([.25,-.25])])}};
        await probe.store.save(probe.controller.owner,1,policy.generation,probe.point);
    }''')
    root_snapshot = root.evaluate('async()=>({state:await probe.store.state(),rows:await probe.store.list()})')
    before_root, before_preview = refreshes(root_peer), refreshes(preview_peer)
    preview.evaluate('probe.controller.configure(true)')
    preview_peer.wait_for_function('(before) => probe.refreshes > before', arg=before_preview)
    assert refreshes(root_peer) == before_root
    preview.evaluate('''async owner => {
        const policy = await probe.store.state();
        const point = {version:1,master:{name:'local-preview-fixture',
            audio:new Blob([new Float32Array([.5,-.5])])}};
        await probe.store.save(owner,1,policy.generation,point);
    }''', original['owner'])
    assert len(preview.evaluate('probe.store.list()')) == 1
    checks.append('recovery starts off independently; policy broadcasts reach only matching root/preview peers')

    locks = root.evaluate('navigator.locks.query()')['held']
    names = {lock['name'] for lock in locks}
    assert 'slimeybeats-recovery:' + original['owner'] in names, names
    assert 'current-work:slimeybeats-recovery:' + isolated['owner'] in names, names
    denied = root_peer.evaluate('''async id => {
        try {await probe.controller.restorePoint(id); return false;}
        catch(error) {return error.message.includes('open in another tab');}
    }''', original['owner'] + ':1')
    assert denied, 'Root live-owner guard must remain intact'
    preview.evaluate('id => probe.controller.restorePoint(id)', original['owner'] + ':1')
    assert preview.evaluate('probe.restored'), 'Root lock incorrectly blocked a preview point'
    preview.evaluate('probe.controller.configure(false,true)')
    assert preview.evaluate('probe.store.list()') == []
    assert root.evaluate('async()=>({state:await probe.store.state(),rows:await probe.store.list()})') == root_snapshot
    assert root.evaluate('async id => Array.from(new Float32Array(await (await probe.store.read(id)).master.audio.arrayBuffer()))',
                         original['owner'] + ':1') == [.25, -.25]
    assert root.evaluate('probe.prefs.state') == {'version': 1, 'advanced': True, 'tutorial': 'completed', 'step': 0}
    checks.append('namespaced held Web Locks preserve live-owner guard; preview restore/delete leaves root policy and audio intact')
    for page in (root, preview, root_peer, preview_peer):
        page.evaluate('probe.controller.dispose(); probe.store.db.close()')
        page.close()


def no_overflow(page):
    size = page.evaluate('''() => ({scroll:document.documentElement.scrollWidth,
        client:document.documentElement.clientWidth,visual:visualViewport.width})''')
    assert size['scroll'] <= size['client'] + 1 and size['client'] <= size['visual'] + 1, size


def ui_checks(browser, base, out, result):
    checks = result['checks']
    mobile = {'is_mobile': True} if not result['browser'].startswith('firefox') else {}
    context = browser.new_context(viewport={'width': 390, 'height': 844}, has_touch=True, **mobile)
    context.on('request', lambda request: result['requests'].append({
        'method': request.method, 'url': request.url, 'body_bytes': len(request.post_data_buffer or b'')}))
    context.add_init_script('''for(const prefix of ['', 'current-work:'])
        localStorage.setItem(prefix+'slimeybeats.interface-preferences.tutorial-dismissed','declined');''')
    context.add_init_script('''window.previewFetches=[]; const originalFetch=window.fetch;
        window.fetch=function(url, options={}) {
            previewFetches.push({url:String(url),method:options.method,cache:options.cache});
            return originalFetch.apply(this,arguments);
        };''')
    page = context.new_page()
    page.on('pageerror', lambda error: result['errors'].append(str(error)))
    page.goto(base + 'current-work/', wait_until='networkidle')
    expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
    expect(page.locator('#preview-banner')).to_be_visible()
    expect(page.locator('#preview-identity')).to_have_text('Build ' + IDENTITY['commit'][:12])
    identity_fetches = page.evaluate("previewFetches.filter(row=>row.url.endsWith('/preview-build.json'))")
    assert identity_fetches == [{'url': base + 'current-work/preview-build.json', 'method': 'GET', 'cache': 'no-store'}]
    asset_versions = page.evaluate('''() => [...document.querySelectorAll('script[src],link[rel=stylesheet]')]
        .map(el=>new URL(el.src||el.href).searchParams.get('v'))''')
    assert set(asset_versions) == {IDENTITY['runtime_version']}, asset_versions
    page.locator('#preview-details > summary').tap()
    expect(page.locator('#preview-commit')).to_have_text(IDENTITY['commit'])
    expect(page.locator('#preview-main-commit')).to_have_text(IDENTITY['main_commit'])
    expect(page.locator('#preview-run')).to_have_attribute('href', IDENTITY['run_url'])
    no_overflow(page)
    page.screenshot(path=str(out / 'preview-build-details-390.png'))
    page.locator('#preview-details > summary').tap()
    audio_supported = page.evaluate('Boolean(globalThis.AudioContext || globalThis.webkitAudioContext)')
    if audio_supported:
        page.locator('#file').set_input_files(fixture('local-preview-fixture.wav', seconds=.3))
        expect(page.locator('#mobile-master')).to_be_enabled(timeout=30000)
        page.locator('#mobile-master').tap()
        expect(page.locator('#export')).to_be_enabled(timeout=120000)
    else:
        result['not_run'] = ['Audio import/master/playback: installed browser has neither AudioContext nor webkitAudioContext; audio APIs were not mocked.']
    for width, height in ((390, 844), (320, 568)):
        page.set_viewport_size({'width': width, 'height': height})
        no_overflow(page)
        page.locator('#preview-details > summary').tap()
        no_overflow(page)
        page.screenshot(path=str(out / f'preview-build-details-{width}.png'))
        page.locator('#preview-details > summary').tap()
        if audio_supported:
            page.locator('#play').tap()
            page.locator('#stop').tap()
        page.locator('#open-learning').tap()
        expect(page.locator('#learning-dialog')).to_be_visible()
        page.locator('#learning-close').tap()
        expect(page.locator('#learning-dialog')).to_be_hidden()
        page.screenshot(path=str(out / f'preview-playback-{width}.png'))
    checks.append('390/320 touch Help reachable with preview banner; metadata details fit without horizontal overflow')
    if audio_supported:
        checks.append('390/320 real audio import/master and touch playback/Stop passed')

    mismatch = {**IDENTITY, 'runtime_version': '0' * 16 if IDENTITY['runtime_version'] != '0' * 16 else 'f' * 16}
    for label, body, status in [('missing', '', 404), ('invalid', '{}', 200),
                                ('mismatch', json.dumps(mismatch), 200)]:
        page.route('**/preview-build.json', lambda route, request, body=body, status=status:
                   route.fulfill(status=status, content_type='application/json', body=body))
        page.reload(wait_until='networkidle')
        expected = 'Build changed; reload to verify identity' if label == 'mismatch' else 'Build identity unavailable'
        expect(page.locator('#preview-identity')).to_have_text(expected)
        expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
        page.locator('#preview-details > summary').tap()
        expect(page.locator('#preview-run')).to_be_hidden()
        no_overflow(page)
        page.unroute('**/preview-build.json')
    checks.append('missing/invalid/stale-runtime metadata never claims a commit; WASM engine still initializes')
    assert not result['errors'], result['errors']
    assert all(row['method'] == 'GET' and not row['body_bytes'] and
               urlparse(row['url']).path.startswith('/current-work/')
               for row in result['requests'] if row['url'].startswith(base)), result['requests']
    assert all(row['url'].startswith(base) or row['url'].startswith('blob:')
               for row in result['requests']), 'Preview requested a remote resource'
    checks.append('identity GET uses no-store; asset versions match metadata; preview requests stay under current-work with no upload')
    previous_requests = len(result['requests'])
    root_page = context.new_page()
    root_page.goto(base + '?preview=true&channel=current-work', wait_until='networkidle')
    expect(root_page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
    if root_page.locator('#preview-banner').count():
        expect(root_page.locator('#preview-banner')).to_be_hidden()
    assert not any('/preview-build.json' in row['url'] for row in result['requests'][previous_requests:])
    checks.append('root app remains unbadged and never requests preview metadata despite query flags')
    context.close()


def run(args):
    args.out.mkdir(parents=True, exist_ok=True)
    result = {'passed': False, 'checks': [], 'requests': [], 'errors': []}
    server = Server(('127.0.0.1', 0), Handler)
    server.preview_site = args.site.resolve()
    server.main_site = (args.main_site or args.site).resolve()
    versions = set(re.findall(r'(?:src|href)="[^"?]+\.(?:js|css)\?v=([a-f0-9]{16})"',
                              (server.preview_site / 'index.html').read_text(encoding='utf-8')))
    assert len(versions) == 1, f'Built index must have one 16-hex runtime version: {versions}'
    IDENTITY['runtime_version'] = versions.pop()
    result['runtime_version'] = IDENTITY['runtime_version']
    threading.Thread(target=server.serve_forever, daemon=True).start()
    base = f'http://127.0.0.1:{server.server_port}/'
    try:
        with sync_playwright() as pw:
            browser = getattr(pw, args.browser).launch(headless=True)
            result['browser'] = args.browser + ' ' + browser.version
            context = browser.new_context()
            storage_checks(context, base, result['checks'])
            # Module directory, rather than query flags, chooses the namespace.
            paths = [('/current-work/preview-channel.js?preview=false', True)]
            if (server.main_site / 'preview-channel.js').exists():
                paths.append(('/preview-channel.js?preview=true', False))
            for path, expected in paths:
                page = context.new_page(); page.goto(base + '__preview_probe__.html')
                assert page.evaluate('async path => (await import(path)).IS_PREVIEW', path) is expected
                page.close()
            result['checks'].append('query parameters cannot select root/preview storage identity')
            context.close()
            ui_checks(browser, base, args.out, result)
            browser.close()
            result['passed'] = True
    finally:
        server.shutdown(); server.server_close()
        (args.out / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps({key: value for key, value in result.items() if key != 'requests'} |
                     {'request_count': len(result['requests'])}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--site', type=Path, default=ROOT / 'web/dist')
    parser.add_argument('--main-site', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    run(parser.parse_args())
