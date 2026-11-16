"""Exercise explicit server consent and real Rust WAV/MP3 jobs in a browser."""
import argparse
from contextlib import closing
import hashlib
import http.client
import json
import math
import os
from pathlib import Path
import secrets
import socket
import struct
import subprocess
import sys
import time
import wave
from urllib.parse import urlsplit

from playwright.sync_api import sync_playwright, expect
from hybrid_test_proxy import start_site

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'rust/tools'))
from verify_exports import independent_measurements


def unused_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--site', type=Path, default=ROOT / 'web/dist')
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    parser.add_argument('--guest', action='store_true', help='Exercise the public switch and scoped guest flow')
    parser.add_argument('--width', type=int, default=390)
    parser.add_argument('--height', type=int, default=844)
    parser.add_argument('--text-scale', type=float, default=1)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    source = args.out / 'synthetic-source.wav'
    with wave.open(str(source), 'wb') as writer:
        writer.setparams((2, 2, 16000, 0, 'NONE', 'not compressed'))
        writer.writeframes(b''.join(struct.pack('<hh', value, value // 2)
            for frame in range(16000 * 4)
            for value in [int(5000 * math.sin(2 * math.pi * 220 * frame / 16000))]))
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    token, api_port = secrets.token_urlsafe(36), unused_port()
    site = start_site(args.site.resolve(), api_port)
    origin = f'http://127.0.0.1:{site.server_port}'
    env = dict(os.environ, SLIMEYBEATS_HYBRID_TOKEN=token,
               SLIMEYBEATS_HYBRID_PUBLIC='1' if args.guest else '0',
               SLIMEYBEATS_HYBRID_ORIGINS=origin, PYTHONDONTWRITEBYTECODE='1')
    result = {'browser': args.browser, 'access': 'guest' if args.guest else 'operator', 'checks': [], 'errors': []}
    process = None
    try:
        with (args.out / 'service.log').open('wb') as log:
            process = subprocess.Popen([sys.executable, '-m', 'hybrid.server', '--cli',
                str(args.cli.resolve()), '--data-dir', str((args.out / 'private-jobs').resolve()),
                '--port', str(api_port)], cwd=ROOT, env=env, stdin=subprocess.DEVNULL,
                stdout=log, stderr=log)
            deadline = time.monotonic() + 20
            while True:
                try:
                    with closing(http.client.HTTPConnection('127.0.0.1', api_port, timeout=1)) as remote:
                        remote.request('GET', '/health')
                        if remote.getresponse().status == 200:
                            break
                except OSError:
                    pass
                if time.monotonic() > deadline or process.poll() is not None:
                    raise RuntimeError('Hybrid service did not become ready; inspect private service.log')
                time.sleep(0.1)
            with sync_playwright() as playwright:
                browser = getattr(playwright, args.browser).launch(headless=True)
                try:
                    context = browser.new_context(viewport={'width': args.width, 'height': args.height},
                                                  accept_downloads=True)
                    if args.guest and args.browser == 'webkit':
                        # Playwright 1.57 WebKit omits Blob bodies from intercepted requests.
                        # Materialize the actual selected Blob, without substituting fixture bytes.
                        # Native Blob transport is separately covered by the direct operator run.
                        context.add_init_script(r'''(() => {
                            const nativeFetch = globalThis.fetch.bind(globalThis);
                            globalThis.fetch = (input, options) => {
                                const endpoint = document.querySelector('#hybrid-endpoint')?.value;
                                const url = new URL(typeof input === 'string' ? input : input.url, location.href);
                                if (endpoint && url.origin === new URL(endpoint).origin
                                    && /^\/v1\/jobs\/[a-f0-9]{32}\/upload$/.test(url.pathname)
                                    && options?.method === 'PUT' && options.body instanceof Blob) {
                                    return options.body.arrayBuffer().then(body => nativeFetch(input, {...options, body}));
                                }
                                return nativeFetch(input, options);
                            };
                        })()''')
                    page = context.new_page()
                    requests = []
                    page.on('pageerror', lambda error: result['errors'].append(error.stack or str(error)))
                    page.on('request', lambda request: requests.append((request.method, request.url)))
                    # Route only the exact configured public API to the owned fixture;
                    # keep the production CSP and visible destination unchanged.
                    availability = {'status': 'online'}
                    def public_api(route):
                        if not args.guest:
                            route.abort()
                            return
                        parsed = urlsplit(route.request.url)
                        if parsed.path == '/v1/status' and availability['status'] != 'online':
                            value = availability['status']
                            status = value if isinstance(value, int) else 200 if value in ('disabled', 'incompatible') else 503
                            body = {'api_version': 1, 'status': 'disabled', 'guest_access': False} if value == 'disabled' else (
                                {'api_version': 2, 'status': 'online', 'guest_access': True} if value == 'incompatible' else {'error': 'fixture unavailable'})
                            route.fulfill(status=status, json=body,
                                headers={'Access-Control-Allow-Origin': origin})
                            return
                        payload = route.request.post_data_buffer
                        if route.request.method in ('POST', 'PUT') and payload is None:
                            raise AssertionError(f'Fixture interception lost {route.request.method} body for {parsed.path}')
                        response = route.fetch(url=f'{origin}{parsed.path}' +
                            (f'?{parsed.query}' if parsed.query else ''),
                            headers={**route.request.headers, 'Origin': origin}, post_data=payload)
                        if parsed.path.endswith('/upload') and response.status != 200:
                            raise AssertionError(f'Fixture upload returned HTTP {response.status}; inspect #hybrid-status')
                        route.fulfill(response=response)
                    page.goto(origin, wait_until='networkidle')
                    if page.locator('#learning-dialog[open]').is_visible():
                        page.locator('#tutorial-decline').click()
                    if args.text_scale != 1:
                        page.evaluate('(scale) => document.documentElement.style.fontSize = `${16 * scale}px`', args.text_scale)
                    expect(page.locator('#hybrid-studio')).to_be_visible()
                    api_origin = page.locator('#hybrid-endpoint').input_value()
                    assert urlsplit(api_origin).scheme == 'https'
                    page.route(f'{api_origin}/**', public_api)
                    assert not any('/v1/' in url for _, url in requests), 'Automatic server request'
                    expect(page.get_by_role('switch', name='Use server processing')).not_to_be_checked()
                    expect(page.locator('#hybrid-endpoint')).to_be_hidden()
                    page.screenshot(path=str(args.out / 'switch-local.png'), full_page=True)
                    result['checks'].append('No server requests before explicit connection')
                    if args.guest:
                        # Start with an actual loaded local source; mode-on must adopt
                        # that File object without touching the local player/result.
                        expect(page.locator('#add')).to_be_enabled(timeout=30000)
                        page.locator('#file').set_input_files(str(source))
                        expect(page.locator('#filename')).to_have_text(source.name, timeout=30000)
                        primary = page.locator('#mobile-master') if page.locator('#mobile-master').is_visible() else page.locator('#master')
                        primary.click()
                        expect(page.locator('#export')).to_be_enabled(timeout=30000)
                        local_result = page.locator('#output-metrics').inner_text()
                    if args.guest:
                        availability['status'] = 503
                    page.get_by_role('switch', name='Use server processing').check()
                    if args.guest:
                        expect(page.locator('#hybrid-availability')).to_have_text('[Server down]')
                        availability['status'] = 'online'
                        # Visibility recovery runs the same health-only check as a
                        # timer tick, without an explicit guest connection intent.
                        page.evaluate('document.dispatchEvent(new Event("visibilitychange"))')
                        expect(page.locator('#hybrid-availability')).to_have_text('[Server online]')
                        expect(page.locator('#hybrid-availability-detail')).to_contain_text('Not connected for processing yet')
                        expect(page.locator('#hybrid-check')).to_have_text('Check & connect')
                        expect(page.locator('#hybrid-format')).to_be_disabled()
                        assert not any('/v1/session' in url for _, url in requests)
                        page.locator('#hybrid-enabled').scroll_into_view_if_needed()
                        page.screenshot(path=str(args.out / 'switch-recovered-needs-connect.png'), full_page=True)
                        page.locator('#hybrid-check').click()
                        result['checks'].append('Initial outage recovery stays health-only and visibly requires explicit Check & connect')
                    if not args.guest:
                        page.locator('#hybrid-advanced > summary').click()
                        page.locator('#hybrid-endpoint').fill(origin)
                        page.locator('#hybrid-token').fill(token)
                        page.locator('#hybrid-connect').click()
                    expect(page.locator('#hybrid-consent')).not_to_be_checked()
                    expect(page.locator('#hybrid-file')).to_be_enabled(timeout=15000)
                    expect(page.locator('#hybrid-availability')).to_have_text('[Server online]', timeout=15000)
                    expect(page.locator('#hybrid-profile option')).not_to_have_count(0, timeout=15000)
                    assert page.locator('#hybrid-token').input_value() == ''
                    if args.guest:
                        expect(page.locator('#hybrid-selection')).to_contain_text(source.name)
                        expect(page.locator('#hybrid-endpoint')).to_be_hidden()
                        # A server-only main selection deliberately cannot decode as
                        # audio; the previously loaded local source must remain intact.
                        page.locator('#file').set_input_files({'name': 'long-server-only.mp3',
                            'mimeType': 'audio/mpeg', 'buffer': b'no local decode should be attempted'})
                        expect(page.locator('#hybrid-selection')).to_contain_text('long-server-only.mp3')
                        expect(page.locator('#filename')).to_have_text(source.name)
                        assert page.locator('#output-metrics').inner_text() == local_result
                        assert not page.locator('#import-error').is_visible()
                        page.locator('#file').set_input_files(str(source))
                        primary = page.locator('#mobile-master') if page.locator('#mobile-master').is_visible() else page.locator('#master')
                        primary.click()
                        expect(page.locator('#hybrid-consent')).to_be_focused()
                        assert page.evaluate('document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1')
                        page.screenshot(path=str(args.out / 'switch-review.png'), full_page=True)
                        assert not any('/v1/jobs' in url for _, url in requests)
                        result['checks'].append('Current main File adopted; server-mode import bypasses decoding; Master opens consent')
                        for status, label in ((401, '[Access needed]'), (403, '[Access needed]'),
                                              (429, '[Server busy]'), (503, '[Server down]'),
                                              ('disabled', '[Server disabled]'), ('incompatible', '[Setup needed]')):
                            availability['status'] = status
                            page.locator('#hybrid-check').click()
                            expect(page.locator('#hybrid-availability')).to_have_text(label)
                            if status == 'disabled':
                                expect(page.locator('#hybrid-availability-detail')).to_contain_text('reachable, but public processing is disabled')
                                expect(page.locator('#hybrid-consent')).to_be_disabled()
                                expect(page.locator('#hybrid-submit')).to_be_disabled()
                                page.locator('#hybrid-enabled').scroll_into_view_if_needed()
                                page.screenshot(path=str(args.out / 'switch-server-disabled.png'), full_page=True)
                                assert not any('/v1/jobs' in url for _, url in requests)
                        page.evaluate('window.dispatchEvent(new Event("offline"))')
                        expect(page.locator('#hybrid-availability')).to_have_text('[Offline]')
                        availability['status'] = 'online'
                        page.evaluate('window.dispatchEvent(new Event("online"))')
                        expect(page.locator('#hybrid-availability')).to_have_text('[Server online]')
                        result['checks'].append('UI distinguishes disabled/setup from 401/403, 429, 503 and offline; disabled blocks audio')
                    for output_format in ('wav24', 'mp3'):
                        page.locator('#hybrid-file').set_input_files(str(source))
                        page.locator('#hybrid-format').select_option(output_format)
                        expect(page.locator('#hybrid-submit')).to_be_disabled()
                        assert not any(method == 'POST' and url.endswith('/v1/jobs')
                                       for method, url in requests), 'Upload without consent'
                        page.locator('#hybrid-consent').check()
                        page.locator('#hybrid-submit').click()
                        expect(page.locator('#hybrid-job-id')).to_contain_text('completed', timeout=180000)
                        buttons = page.locator('#hybrid-artifacts button')
                        expect(buttons).to_have_count(2, timeout=180000)
                        if args.guest:
                            page.get_by_role('switch', name='Use server processing').uncheck()
                            expect(page.locator('#hybrid-panel')).to_be_hidden()
                            expect(page.locator('#hybrid-job')).to_be_visible()
                            expect(page.locator('#hybrid-delete')).to_be_enabled()
                            assert page.locator('#output-metrics').inner_text() == local_result
                            expect(page.locator('#filename')).to_have_text(source.name)
                            expect(page.locator('#master-source')).to_be_enabled()
                        downloads = []
                        for index in range(2):
                            with page.expect_download() as downloaded:
                                buttons.nth(index).click()
                            item = downloaded.value
                            destination = args.out / f'{output_format}-{item.suggested_filename}'
                            item.save_as(destination)
                            downloads.append(destination)
                        audio = next(path for path in downloads if path.suffix in ('.wav', '.mp3'))
                        report = next(path for path in downloads if path.suffix == '.json')
                        lufs, peak = independent_measurements(audio)
                        assert math.isfinite(lufs) and peak <= -0.9, (lufs, peak)
                        document = json.loads(report.read_text(encoding='utf-8'))
                        text = json.dumps(document)
                        assert token not in text and str(args.out.resolve()) not in text
                        result['checks'].append({'format': output_format, 'saved_decode_peak_dbtp': peak,
                                                 'saved_decode_lufs': lufs, 'report_downloaded': True})
                        if output_format == 'mp3':
                            page.screenshot(path=str(args.out / 'server-mobile-complete.png'), full_page=True)
                            page.locator('#hybrid-job').screenshot(path=str(args.out / 'server-job-panel.png'))
                        if args.guest and output_format == 'wav24':
                            page.get_by_role('switch', name='Use server processing').check()
                            availability['status'] = 'down'
                            page.locator('#hybrid-check').click()
                            expect(page.locator('#hybrid-availability')).to_have_text('[Server down]')
                            expect(page.locator('#hybrid-job-id')).to_contain_text('completed')
                            availability['status'] = 'online'
                            count = sum('/v1/jobs' in url and method == 'POST' for method, url in requests)
                            page.locator('#hybrid-check').click()
                            expect(page.locator('#hybrid-availability')).to_have_text('[Server online]')
                            assert count == sum('/v1/jobs' in url and method == 'POST' for method, url in requests)
                            result['checks'].append('Outage/recovery preserves job and never retries audio; downloads work with mode off')
                        page.locator('#hybrid-delete').click()
                        expect(page.locator('#hybrid-job')).to_be_hidden(timeout=15000)
                        requests.clear()
                        if args.guest and output_format != 'mp3':
                            page.get_by_role('switch', name='Use server processing').check()
                    result['checks'].append('Both real jobs explicitly deleted after downloads')
                    storage = page.evaluate('JSON.stringify({local:{...localStorage},session:{...sessionStorage}})')
                    assert token not in storage
                    if not page.locator('#hybrid-advanced').get_attribute('open') == '':
                        page.locator('#hybrid-advanced > summary').click()
                    page.locator('#hybrid-disconnect').click()
                    assert page.locator('#hybrid-token').input_value() == ''
                    expect(page.locator('#hybrid-submit')).to_be_disabled()
                    assert page.evaluate('document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1')
                    assert hashlib.sha256(source.read_bytes()).hexdigest() == digest
                    result['checks'].append('Mobile fit, session-only token, disconnect and immutable source')
                    if args.guest:
                        # Independent server controls must still work when the local
                        # app module never initializes its picker or Master handlers.
                        failed = context.new_page()
                        failed.route('**/app.js*', lambda route: route.abort())
                        failed.route(f'{api_origin}/**', public_api)
                        failed.goto(origin, wait_until='networkidle')
                        expect(failed.locator('#engine-status')).to_have_text('Engine unavailable')
                        expect(failed.get_by_role('switch', name='Use server processing')).not_to_be_checked()
                        failed.get_by_role('switch', name='Use server processing').check()
                        failed.locator('#file').set_input_files(str(source))
                        expect(failed.locator('#hybrid-consent')).to_be_enabled(timeout=15000)
                        expect(failed.locator('#hybrid-selection')).to_contain_text(source.name)
                        failed.get_by_role('switch', name='Use server processing').uncheck()
                        expect(failed.locator('#master')).to_be_disabled()
                        expect(failed.locator('#add')).to_be_disabled()
                        failed.locator('#hybrid-disconnect').evaluate('(node) => node.closest("details").open = true')
                        failed.locator('#hybrid-disconnect').click()
                        failed.close()
                        result['checks'].append('Fresh page defaults local; guest controls work with local engine startup failure')
                    assert not result['errors'], result['errors']
                    result['success'] = True
                finally:
                    browser.close()
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        site.shutdown()
        site.server_close()
        (args.out / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
