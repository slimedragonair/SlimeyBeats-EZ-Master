"""Independent generated-audio regressions against a built Studio WASM/worker/UI.

Use --worker-only for the quick DSP checkpoint. The default also requires the
integrated timeline UI, including numeric/mouse and 320/390px touch controls.
No personal tracks, external requests, CSP changes, or physical-device claims.
"""
import argparse
import base64
import functools
import hashlib
import http.server
import io
import json
from pathlib import Path
import re
import struct
import threading
from urllib.parse import urljoin, urlparse
import zipfile

from playwright.sync_api import expect, sync_playwright
from browser_capture import BrowserDiagnostics

ROOT = Path(__file__).resolve().parents[2]
TOOLS = Path(__file__).resolve().parent
PROBES = {f'/__region_tests/{name}': TOOLS / name for name in
          ['region_edit_checks.js', 'region_edit_pcm_checks.js', 'region_edit_ui_probe.js', 'region_edit_timeline_checks.js',
           'studio_worker_buffer_checks.js']}


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def translate_path(self, path):
        probe = PROBES.get(urlparse(path).path)
        return str(probe) if probe else super().translate_path(path)


def float_wave(raw):
    """Read RIFF chunks independently, preserving exact float PCM bytes."""
    assert raw[:4] == b'RIFF' and raw[8:12] == b'WAVE', 'Not a WAV export'
    at, layout, pcm = 12, None, None
    while at + 8 <= len(raw):
        name, size = struct.unpack_from('<4sI', raw, at)
        body = at + 8
        assert body + size <= len(raw), 'Truncated WAV'
        if name == b'fmt ':
            encoding, channels, rate, _, align, bits = struct.unpack_from('<HHIIHH', raw, body)
            if encoding == 0xfffe:
                assert size >= 40 and struct.unpack_from('<H', raw, body + 16)[0] >= 22
                assert raw[body + 24:body + 40] == bytes.fromhex('0300000000001000800000aa00389b71')
                encoding = 3
            assert encoding == 3 and bits == 32 and align == channels * 4, 'Expected float32 PCM'
            layout = channels, rate
        if name == b'data':
            pcm = raw[body:body + size]
        at = body + size + (size & 1)
    assert layout and pcm is not None and len(pcm) % (layout[0] * 4) == 0
    return layout, pcm


def verify_artifacts(artifacts, out):
    original = {name: base64.b64decode(value, validate=True) for name, value in artifacts['original'].items()}
    checks = []
    for kind in ['raw', 'bundle']:
        raw = base64.b64decode(artifacts[kind], validate=True)
        (out / f'worker-{kind}.zip').write_bytes(raw)
        archive = zipfile.ZipFile(io.BytesIO(raw))
        assert archive.testzip() is None
        assert len(archive.namelist()) == len(set(archive.namelist())), 'Duplicate ZIP members'
        for name in original:
            assert name in archive.namelist(), f'Missing edited bank member {name}'
        assert archive.read('part_9.wav') == original['part_9.wav'], 'Export changed other stem'
        layout, before = float_wave(original['vocals.wav'])
        after_layout, after = float_wave(archive.read('vocals.wav'))
        assert layout == after_layout
        channels, rate = layout
        edit = artifacts['edit']
        start = int(edit['start_seconds'] * rate + .5) * channels * 4
        end = int(edit['end_seconds'] * rate + .5) * channels * 4
        stride = channels * 4
        expected = before[:start] + b''.join(before[i:i + stride] for i in range(end - stride, start - stride, -stride)) + before[end:]
        assert after == expected and after != before, f'{kind}: edited bank/channel order differs'
        if kind == 'bundle':
            assert archive.read('mix.wav') == base64.b64decode(artifacts['preview']), 'Bundle preview mismatch'
            assert json.loads(archive.read('session.json'))['info']['preview_is_mix'] is True
        checks.append(f'Python independently extracts {kind} ZIP: exact edited float32 bank/channel order, untouched stem' +
                      (', committed preview' if kind == 'bundle' else ''))
    return checks


def runtime_urls(site, base):
    source = (site / 'studio-worker.js').read_text(encoding='utf-8')
    match = re.search(r'from\s*[\"\']([^\"\']*/pkg/dsp\.js[^\"\']*)[\"\']', source)
    if not match:
        raise RuntimeError('Cannot find the built Studio worker WASM import')
    return {'engineUrl': urljoin(base + 'studio-worker.js', match.group(1))}


def run(args):
    args.site, args.out = args.site.resolve(), args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    result = {'passed': False, 'checks': [], 'errors': [], 'requests': [],
              'scope': 'Headless browser with generated fixtures. Touch means emulated browser input; physical phone/audio unqualified.',
              'site': str(args.site), 'worker': 'pending', 'ui': 'not_run' if args.worker_only else 'pending'}
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    base = f'http://127.0.0.1:{server.server_address[1]}/'
    page, diagnostics = None, None
    try:
        with sync_playwright() as playwright:
            browser = getattr(playwright, args.browser).launch(headless=True,
                **({'executable_path': args.executable} if args.executable else {}))
            result['browser'] = args.browser + ' ' + browser.version
            context = browser.new_context(viewport={'width': 1480, 'height': 1000}, accept_downloads=True)
            def request(route):
                request = route.request
                # WebKit services Blob.arrayBuffer() through a same-origin blob
                # GET during worker recovery. This stays entirely in memory.
                local = request.url.startswith(base) or request.url.startswith('blob:' + base)
                allowed = request.method == 'GET' and not request.post_data_buffer and local
                result['requests'].append({'url': request.url, 'method': request.method, 'allowed': allowed})
                route.continue_() if allowed else route.abort()
            context.route('**/*', request)
            page = context.new_page()
            diagnostics = BrowserDiagnostics(page, args.browser, result)
            try:
                page.goto(base, wait_until='networkidle')
                expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
                csp = page.locator('meta[http-equiv="Content-Security-Policy"]').get_attribute('content')
                assert csp and "'unsafe-inline'" not in csp and "'unsafe-eval'" not in csp, 'Production CSP must stay strict'
                result['csp'] = csp
                urls = runtime_urls(args.site, base)
                urls['clientUrl'] = page.evaluate("""() => {
                    const entry = performance.getEntriesByType('resource').find(e => new URL(e.name).pathname.endsWith('/studio-client.js'));
                    if (!entry) throw Error('Production StudioClient was not loaded by the application');
                    return entry.name;
                }""")
                result['runtime'] = urls
                urls['playbackUrl'] = page.evaluate("""() => {
                    const entry = performance.getEntriesByType('resource').find(e => new URL(e.name).pathname.endsWith('/studio-player.js'));
                    if (!entry) throw Error('Production StudioPlayback was not loaded by the application');
                    return entry.name;
                }""")
                result['wasm_sha256'] = hashlib.sha256((args.site / 'pkg/dsp_bg.wasm').read_bytes()).hexdigest()
                worker = page.evaluate("""async urls => {
                    const { verifyRegionEdits } = await import('/__region_tests/region_edit_checks.js');
                    return verifyRegionEdits(urls);
                }""", urls)
                result['checks'].extend(worker['checks'])
                result['checks'].extend(verify_artifacts(worker['artifacts'], args.out))
                replacement = page.evaluate("""async urls => {
                    const { verifyWorkerBufferReplacement } = await import('/__region_tests/studio_worker_buffer_checks.js');
                    return verifyWorkerBufferReplacement(urls);
                }""", urls)
                result['checks'].extend(replacement['checks'])
                result['worker_buffer_replacement'] = replacement
                result['worker'] = 'passed'
                # Persist the checkpoint before running longer application interactions.
                (args.out / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
                if not args.worker_only:
                    result['checks'].extend(page.evaluate("""async clientUrl => {
                        const { verifyTimelineEdges } = await import('/__region_tests/region_edit_timeline_checks.js');
                        return verifyTimelineEdges(clientUrl);
                    }""", urls['clientUrl']))
                    from region_edit_ui_checks import verify_ui
                    result['checks'].extend(verify_ui(browser, context, page, base, args.out, diagnostics, args.browser, result, request))
                    result['ui'] = 'passed'
                assert all(r['allowed'] for r in result['requests']), 'Unexpected external/upload request'
                assert not result['errors'], result['errors']
                result['passed'] = True
            except Exception as error:
                result['failure'] = repr(error)
                if result['worker'] == 'pending':
                    result['worker'] = 'failed'
                elif result['ui'] == 'pending':
                    result['ui'] = 'failed'
                if page and not page.is_closed():
                    try:
                        partial = page.evaluate('window.regionEditChecks || []')
                        result['checks'] = list(dict.fromkeys(result['checks'] + partial))
                        diagnostics.capture(args.out / 'failure.png', full_page=True)
                    except Exception as capture_error:
                        result['capture_failure'] = repr(capture_error)
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
    parser.add_argument('--worker-only', action='store_true', help='Quick real WASM/worker checkpoint; UI explicitly not_run')
    args = parser.parse_args()
    for name in ['index.html', 'studio-worker.js', 'pkg/dsp.js', 'pkg/dsp_bg.wasm']:
        if not (args.site / name).is_file():
            parser.error(f'Missing built site asset: {args.site / name}')
    run(args)
