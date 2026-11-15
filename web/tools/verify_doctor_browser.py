"""Verify built Doctor WASM, real workers and Studio UI using synthetic fixtures.

No build runs here. --worker-only explicitly leaves UI unqualified. Browser audio
is synthetic/headless; this is not a physical-device listening qualification.
"""
import argparse
import array
import functools
import hashlib
import http.server
import io
import json
import math
from pathlib import Path
import re
import threading
from urllib.parse import urljoin, urlparse
import wave

from playwright.sync_api import expect, sync_playwright
from browser_capture import BrowserDiagnostics

ROOT = Path(__file__).resolve().parents[2]
TOOLS = Path(__file__).resolve().parent
PROBES = {f'/__doctor_tests/{name}': TOOLS / name for name in
          ['doctor_worker_checks.js', 'doctor_pcm_checks.js', 'doctor_ui_checks.js', 'studio_capture_processor.js']}


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def translate_path(self, path):
        return str(PROBES[urlparse(path).path]) if urlparse(path).path in PROBES else super().translate_path(path)


def fixture(name, clipped=True):
    samples = array.array('h', (round(0.12 * 32767 * math.sin(2 * math.pi * (740 if c else 250) * f / 48000))
                               for f in range(48000 * 4) for c in range(2)))
    if clipped:
        for i, value in enumerate([.25, .6, .9, 1, 1, 1, .9, .6, .25]):
            samples[(48000 + i) * 2] = round(value * 32767)
    output = io.BytesIO()
    with wave.open(output, 'wb') as writer:
        writer.setparams((2, 2, 48000, 0, 'NONE', 'not compressed'))
        writer.writeframes(samples.tobytes())
    return {'name': name, 'mimeType': 'audio/wav', 'buffer': output.getvalue()}


def ready(page):
    expect(page.locator('#s-export')).to_be_enabled(timeout=60000)


def scan(page):
    page.locator('#s-doctor-scan').click()
    expect(page.locator('#s-doctor-status')).to_contain_text('Scanning changed no audio.', timeout=60000)


def dismiss_welcome(page):
    # A fresh mobile context can receive the optional first-run tour. Use its
    # ordinary close action before exercising controls behind the modal.
    if page.locator('#learning-dialog').is_visible():
        page.locator('#learning-close').click()
        expect(page.locator('#learning-dialog')).not_to_be_visible()


def ui_checks(page, diagnostics, out):
    checks = []
    dismiss_welcome(page)
    page.evaluate("async () => (await import('/__doctor_tests/doctor_ui_checks.js')).installDoctorProbe()")
    page.locator('#tab-studio-tools').click()
    page.locator('.edit-import summary').click()
    for kind, clipped in [('vocals', True), ('instrumental', False)]:
        page.locator(f'#s-stem-files input[data-kind="{kind}"]').set_input_files(fixture(f'Doctor {kind}.wav', clipped))
    page.locator('#s-import').click()
    ready(page)
    page.locator('#s-tab-doctor').click()
    page.locator('#s-target').select_option('vocals')
    page.evaluate('doctorUi.remember()')
    scan(page)
    page.evaluate('doctorUi.unchanged()')
    expect(page.locator('#s-doctor-target')).to_contain_text('vocals')
    assert page.locator('#s-doctor-issues button').count() > 0
    expect(page.locator('#s-doctor-issues button').first).to_contain_text('3 frames per channel')
    expect(page.locator('#s-doctor-legend')).to_contain_text('Clipping candidate')
    expect(page.locator('#s-doctor-legend')).to_contain_text('Low-mid buildup')
    expect(page.locator('#s-doctor-marker-label')).to_contain_text('do not redraw the selected stem')
    assert page.locator('#s-doctor-markers button').count() == page.locator('#s-doctor-issues button').count()
    page.locator('#s-doctor-issues button').first.click()
    page.evaluate('doctorUi.unchanged()')
    assert abs(page.evaluate('doctorUi.playback.position()') - 1) < .01
    expect(page.locator('#s-doctor-fix')).to_be_enabled()
    with page.expect_download() as event:
        page.locator('#s-doctor-download').click()
    report_file = out / 'issue-report.json'
    event.value.save_as(str(report_file))
    report = json.loads(report_file.read_text(encoding='utf-8'))
    assert report['target'] == 'vocals' and report['report']['schema_version'] == 1
    assert not any(key in report for key in ['audio', 'samples', 'recipe', 'undo'])
    assert 'separate from session ZIPs and recovery' in page.locator('#s-doctor-page').inner_text()
    checks.append('UI scan/list/marker seek and local JSON download leave exact PCM/history/recovery unchanged; report scope is explicit')

    page.locator('#s-play').click()
    page.wait_for_function('() => doctorUi.playback.playing')
    page.evaluate('doctorUi.positionBefore = doctorUi.playback.position()')
    page.locator('#s-doctor-fix').click()
    ready(page)
    page.wait_for_function('() => doctorUi.playback.playing')
    assert page.evaluate('doctorUi.playback.position() >= doctorUi.positionBefore - 0.01')
    expect(page.locator('#s-doctor-issues')).to_be_empty()
    expect(page.locator('#s-doctor-download')).to_be_disabled()
    page.locator('#s-play').click()
    expect(page.locator('#s-apply-stem')).to_be_enabled()
    scan(page)
    page.locator('#s-undo').click()
    ready(page)
    expect(page.locator('#s-doctor-issues')).to_be_empty()
    scan(page)
    page.locator('#s-redo').click()
    ready(page)
    expect(page.locator('#s-doctor-issues')).to_be_empty()
    scan(page)
    page.locator('#s-apply-stem').click()
    ready(page)
    expect(page.locator('#s-doctor-issues')).to_be_empty()
    page.locator('#s-undo').click()
    ready(page)
    checks.append('UI fix keeps playing transport position; preview, undo/redo and bank changes invalidate reports; stem apply stays explicit')

    page.evaluate('doctorUi.holdScan = true')
    page.locator('#s-doctor-scan').click()
    page.wait_for_function('() => doctorUi.held === true')
    page.locator('#s-target').select_option('preview')
    page.evaluate('doctorUi.holdScan = false; doctorUi.release()')
    expect(page.locator('#s-doctor-scan')).to_be_enabled()
    expect(page.locator('#s-doctor-issues')).to_be_empty()
    expect(page.locator('#s-doctor-download')).to_be_disabled()
    page.locator('#s-target').select_option('vocals')
    scan(page)
    page.locator('#s-doctor-issues button').first.click()
    page.locator('#s-tab-mix').click()
    for mute in page.locator('#s-faders input[aria-label$=" mute"]').all():
        mute.check()
    page.locator('#s-tab-doctor').click()
    expect(page.locator('#s-doctor-issues')).to_be_empty()
    scan(page)
    page.locator('#s-doctor-issues button').first.click()
    expect(page.locator('#s-doctor-fix')).to_be_disabled()
    before = page.evaluate('doctorUi.mutations()')
    page.evaluate('document.getElementById("s-doctor-fix").onclick()')
    assert page.evaluate('doctorUi.mutations()') == before, 'Direct handler discarded pending mix'
    expect(page.locator('#s-doctor-target')).to_contain_text('Live fader changes are pending')
    checks.append('Late worker result rejected after target change; pending live-mix report labels saved audio and repair handler refuses bypass')
    page.locator('#s-tab-mix').click()
    page.locator('#s-mix').click()
    ready(page)
    page.locator('#s-tab-doctor').click()
    page.locator('#s-target').select_option('preview')
    scan(page)
    assert page.locator('#s-doctor-issues button').count() > 0
    page.locator('#s-doctor-issues button').first.click()
    expect(page.locator('#s-doctor-fix')).to_be_disabled()
    page.evaluate('doctorUi.startCapture()')
    page.locator('#s-play').click()
    page.wait_for_function('() => doctorUi.playback.playing')
    scan(page)
    page.wait_for_function('() => doctorUi.blocks.length > 20')
    page.evaluate('doctorUi.captureSince = doctorUi.playback.context().currentTime + 0.1')
    page.wait_for_function('() => doctorUi.blocks.some(b => b.time > doctorUi.captureSince + 0.1)')
    capture = page.evaluate('doctorUi.assertSilent()')
    page.evaluate('doctorUi.stopCapture()')
    page.locator('#s-play').click()
    assert capture['samples'] > 0
    checks.append('All-muted saved preview stays silent through read-only scanning during playback, measured from actual output worklet PCM')
    return checks


def layout_checks(browser, base, out, request, browser_name, result):
    checks = []
    for width in [320, 390]:
        context = browser.new_context(viewport={'width': width, 'height': 844}, has_touch=True)
        context.route('**/*', request)
        page = context.new_page()
        diagnostics = BrowserDiagnostics(page, browser_name, result)
        try:
            page.goto(base, wait_until='networkidle')
            expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
            dismiss_welcome(page)
            page.locator('#tab-studio-tools').click()
            page.locator('#s-file').set_input_files(fixture('Doctor layout.wav'))
            ready(page)
            page.locator('#s-tab-doctor').click()
            scan(page)
            for scale in [1, 2]:
                page.evaluate('(scale) => document.documentElement.style.fontSize = `${scale * 100}%`', scale)
                page.wait_for_function('''scale => Math.abs(parseFloat(getComputedStyle(document.documentElement).fontSize) - 16 * scale) < 0.01 &&
                    (scale === 1 || document.body.classList.contains('large-text'))''', arg=scale)
                expect(page.locator('#s-wave')).to_be_visible()
                expect(page.locator('#s-playback-details')).to_have_attribute('open', '')
                page.locator('#s-doctor-issues button').first.scroll_into_view_if_needed()
                page.locator('#s-doctor-issues button').first.tap()
                size = page.evaluate('({scroll: document.documentElement.scrollWidth, client: document.documentElement.clientWidth, visual: visualViewport.width})')
                if size['scroll'] > size['client'] + 1 or size['scroll'] > size['visual'] + 1:
                    size['overflow'] = page.evaluate('''() => [...document.querySelectorAll('body *')].map(el => {
                        const r = el.getBoundingClientRect(), s = getComputedStyle(el);
                        return {tag: el.tagName, id: el.id, className: el.className, left:r.left, right:r.right,
                            width:r.width, scroll:el.scrollWidth, client:el.clientWidth,
                            fontSize:s.fontSize, whiteSpace:s.whiteSpace, overflow:s.overflowX};
                    }).filter(r => r.width > 0 && (r.left < -1 || r.right > document.documentElement.clientWidth + 1 ||
                        (r.scroll > r.client + 1 && r.overflow === 'visible'))).slice(0, 80)''')
                    (out / f'doctor-{width}-{scale * 100}-geometry.json').write_text(json.dumps(size, indent=2), encoding='utf-8')
                    diagnostics.capture(out / f'doctor-{width}-{scale * 100}-failure.png', full_page=True)
                    raise AssertionError(size)
                expect(page.locator('#s-doctor-fix')).to_be_enabled()
                diagnostics.capture(out / f'doctor-{width}-{scale * 100}.png', full_page=True)
                checks.append(f'{width}px with {scale * 100}% text: measured no document overflow; real issue touch/selection and fix control reachable')
        finally:
            context.close()
    return checks


def run(args):
    args.site, args.out = args.site.resolve(), args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    result = {'passed': False, 'checks': [], 'errors': [], 'requests': [], 'site': str(args.site),
              'worker': 'pending', 'ui': 'not_run' if args.worker_only else 'pending',
              'scope': 'Built WASM/real worker and headless browser output PCM; emulated touch only. No physical-device or listening qualification.'}
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    base = f'http://127.0.0.1:{server.server_address[1]}/'
    try:
        with sync_playwright() as p:
            browser = getattr(p, args.browser).launch(headless=True, **({'executable_path': args.executable} if args.executable else {}))
            context = browser.new_context(accept_downloads=True)
            def request(route):
                value = route.request
                # WebKit routes recovery Blob.arrayBuffer() reads through this
                # hook. Permit this fixture origin's Blob GETs, never uploads or
                # Blob URLs belonging to another origin.
                allowed = value.method == 'GET' and not value.post_data_buffer and (
                    value.url.startswith(base) or value.url.startswith('blob:' + base))
                result['requests'].append({'url': value.url, 'method': value.method, 'allowed': allowed})
                route.continue_() if allowed else route.abort()
            context.route('**/*', request)
            page = context.new_page()
            diagnostics = BrowserDiagnostics(page, args.browser, result)
            try:
                page.goto(base, wait_until='networkidle')
                expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
                csp = page.locator('meta[http-equiv="Content-Security-Policy"]').get_attribute('content')
                assert csp and "'unsafe-inline'" not in csp and "'unsafe-eval'" not in csp
                source = (args.site / 'studio-worker.js').read_text(encoding='utf-8')
                match = re.search(r'from\s*[\"\']([^\"\']*/pkg/dsp\.js[^\"\']*)[\"\']', source)
                assert match, 'Built worker WASM import missing'
                urls = {'engineUrl': urljoin(base + 'studio-worker.js', match.group(1))}
                for key, name in [('clientUrl', 'studio-client.js'), ('playbackUrl', 'studio-player.js')]:
                    urls[key] = page.evaluate('(name) => performance.getEntriesByType("resource").find(e => new URL(e.name).pathname.endsWith(`/${name}`)).name', name)
                result['runtime'] = urls
                result['wasm_sha256'] = hashlib.sha256((args.site / 'pkg/dsp_bg.wasm').read_bytes()).hexdigest()
                result['checks'].extend(page.evaluate('async urls => (await import("/__doctor_tests/doctor_worker_checks.js")).verifyDoctor(urls)', urls)['checks'])
                result['worker'] = 'passed'
                if not args.worker_only:
                    result['checks'].extend(ui_checks(page, diagnostics, args.out))
                    result['checks'].extend(layout_checks(browser, base, args.out, request, args.browser, result))
                    result['ui'] = 'passed'
                assert all(item['allowed'] for item in result['requests'])
                assert not result['errors'], result['errors']
                result['passed'] = True
            except Exception as error:
                result['failure'] = repr(error)
                if result['worker'] == 'pending':
                    result['worker'] = 'failed'
                elif result['ui'] == 'pending':
                    result['ui'] = 'failed'
                result['checks'] = list(dict.fromkeys(result['checks'] + page.evaluate('window.doctorChecks || []')))
                diagnostics.capture(args.out / 'failure.png', full_page=True)
                raise
            finally:
                context.close()
                browser.close()
    finally:
        server.shutdown()
        server.server_close()
        (args.out / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps({key: value for key, value in result.items() if key != 'requests'}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--site', type=Path, default=ROOT / 'web/dist')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    parser.add_argument('--executable')
    parser.add_argument('--worker-only', action='store_true')
    args = parser.parse_args()
    for name in ['index.html', 'studio-worker.js', 'pkg/dsp.js', 'pkg/dsp_bg.wasm']:
        if not (args.site / name).is_file():
            parser.error(f'Missing built site asset: {args.site / name}')
    run(args)
