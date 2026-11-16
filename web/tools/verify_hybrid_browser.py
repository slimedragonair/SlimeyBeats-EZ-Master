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
               SLIMEYBEATS_HYBRID_ORIGINS=origin, PYTHONDONTWRITEBYTECODE='1')
    result = {'browser': args.browser, 'checks': [], 'errors': []}
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
                    context = browser.new_context(viewport={'width': 390, 'height': 844},
                                                  accept_downloads=True)
                    page = context.new_page()
                    requests = []
                    page.on('pageerror', lambda error: result['errors'].append(str(error)))
                    page.on('request', lambda request: requests.append((request.method, request.url)))
                    page.goto(origin, wait_until='networkidle')
                    if page.locator('#learning-dialog[open]').is_visible():
                        page.locator('#tutorial-decline').click()
                    expect(page.locator('#hybrid-studio')).to_be_visible()
                    assert not any('/v1/' in url for _, url in requests), 'Automatic server request'
                    result['checks'].append('No server requests before explicit connection')
                    page.locator('#hybrid-studio > summary').click()
                    page.locator('#hybrid-endpoint').fill(origin)
                    page.locator('#hybrid-token').fill(token)
                    page.locator('#hybrid-connect').click()
                    expect(page.locator('#hybrid-file')).to_be_enabled(timeout=15000)
                    assert page.locator('#hybrid-token').input_value() == ''
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
                        expect(buttons).to_have_count(2)
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
                        page.locator('#hybrid-delete').click()
                        expect(page.locator('#hybrid-job')).to_be_hidden(timeout=15000)
                        requests.clear()
                    result['checks'].append('Both real jobs explicitly deleted after downloads')
                    storage = page.evaluate('JSON.stringify({local:{...localStorage},session:{...sessionStorage}})')
                    assert token not in storage
                    page.locator('#hybrid-disconnect').click()
                    assert page.locator('#hybrid-token').input_value() == ''
                    expect(page.locator('#hybrid-submit')).to_be_disabled()
                    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth + 1')
                    assert hashlib.sha256(source.read_bytes()).hexdigest() == digest
                    result['checks'].append('Mobile fit, session-only token, disconnect and immutable source')
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
