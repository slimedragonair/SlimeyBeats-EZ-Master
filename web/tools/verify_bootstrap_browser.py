"""Check real app startup and private reporting after a required module fails."""
import argparse
import functools
import http.server
import json
from pathlib import Path
import threading
import time
from urllib.parse import parse_qs, urlparse

from playwright.sync_api import sync_playwright, expect

ROOT = Path(__file__).resolve().parents[2]
PRIVATE_ERROR = r"C:\Users\PRIVATE_BOOTSTRAP_CANARY\private-recording.wav"


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *unused):
        pass


class ProbeServer(http.server.ThreadingHTTPServer):
    # Module fanout must not compete for the stdlib server's five pending sockets.
    request_queue_size = 64


def reporting_ready(page):
    # Keep this in the driver; no evaluated predicate or relaxed CSP is needed.
    for _ in range(400):
        if page.locator('#engine-report').evaluate('el=>typeof el.onclick === "function"'):
            return
        page.wait_for_timeout(25)
    raise AssertionError('Independent reporting did not initialize')


def run(args):
    result = {'cases': [], 'requests': [], 'success': False}
    server = ProbeServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    base = f'http://127.0.0.1:{server.server_address[1]}/'
    try:
        with sync_playwright() as p:
            browser = getattr(p, args.browser).launch(headless=True)
            result['browser'] = args.browser + ' ' + browser.version
            try:
                for mode in ['successful_load', 'blocked_dependency', 'private_exception', 'startup_deadline']:
                    context = browser.new_context(accept_downloads=True)
                    context.add_init_script("localStorage.setItem('slimeybeats.interface-preferences','{corrupt')")
                    page = context.new_page()
                    held = []
                    errors, console, failed = [], [], []
                    page.on('pageerror', lambda error: errors.append(str(error)))
                    page.on('console', lambda message: console.append(message.text) if message.type == 'error' else None)
                    page.on('requestfailed', lambda request: failed.append({'url': request.url, 'failure': request.failure}))
                    page.on('request', lambda request: result['requests'].append({
                        'method': request.method, 'url': request.url, 'body_bytes': len(request.post_data_buffer or b'')}))
                    if mode == 'blocked_dependency':
                        page.route('**/session-export-client.js*', lambda route: route.abort('connectionfailed'))
                    elif mode == 'startup_deadline':
                        page.route('**/session-export-client.js*', lambda route: held.append(route))
                    elif mode == 'private_exception':
                        code = 'export function packageSession() {}\nthrow new Error(' + json.dumps(PRIVATE_ERROR) + ');'
                        page.route('**/session-export-client.js*', lambda route: route.fulfill(
                            status=200, content_type='text/javascript', body=code))
                    case = {'mode': mode}
                    result['cases'].append(case)
                    try:
                        started = time.monotonic()
                        page.goto(base, wait_until='commit' if mode == 'startup_deadline' else 'networkidle')
                        if mode == 'startup_deadline':
                            for _ in range(400):
                                if held:
                                    break
                                page.wait_for_timeout(25)
                            assert held, 'Bootstrap did not request its required dependency'
                            reporting_ready(page)
                        if mode == 'successful_load':
                            expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
                            assert page.locator('html').get_attribute('data-engine-state') == 'ready'
                            expect(page.locator('#ui-storage-notice')).to_contain_text('could not be read')
                            assert not failed and not console, (failed, console)
                        else:
                            expect(page.locator('#engine-status')).to_have_text('Engine unavailable',
                                timeout=35000 if mode == 'startup_deadline' else 10000)
                            assert page.locator('html').get_attribute('data-engine-state') == 'failed'
                            expect(page.locator('#status')).to_contain_text('Reload')
                            expect(page.locator('#status')).to_contain_text('Report a problem')
                            expect(page.locator('#engine-report')).to_be_visible()
                            reporting_ready(page)
                            if mode == 'blocked_dependency':
                                assert failed and all(urlparse(item['url']).path == '/session-export-client.js' for item in failed), failed
                            else:
                                assert not failed and not console, (failed, console)
                            if mode == 'startup_deadline':
                                assert held, 'The required dependency must still be pending at the deadline'
                                case['deadline_seconds'] = 30
                                case['elapsed_seconds'] = round(time.monotonic() - started, 3)
                                assert case['elapsed_seconds'] >= 29, 'Startup failed before the intended deadline'
                            page.locator('#engine-report').click()
                            expect(page.locator('#support-dialog')).to_be_visible()
                            page.locator('#support-build').click()
                            preview = page.locator('#support-preview').inner_text()
                            assert 'startup_failed' in preview and 'Engine initialized: no / unavailable' in preview, preview
                            assert 'Nothing was uploaded' in preview
                            for text in [page.locator('body').inner_text(), preview, *console, *errors]:
                                assert 'PRIVATE_BOOTSTRAP_CANARY' not in text and PRIVATE_ERROR not in text
                            assert base not in preview, 'Runtime exception URL entered the support report'
                            with page.expect_download() as download:
                                page.locator('#support-download').click()
                            saved = args.out / f'{mode}-support-report.txt'
                            download.value.save_as(str(saved))
                            assert saved.read_text(encoding='utf-8') == preview
                            case['report_download_matches_preview'] = True
                        assert not errors, errors
                        assert page.evaluate("localStorage.getItem('slimeybeats.interface-preferences')") == '{corrupt'
                        case.update(success=True, page_errors=errors, failed_requests=failed)
                    except Exception:
                        case.update(success=False, page_errors=errors, console_errors=console,
                                    failed_requests=failed, engine=page.locator('#engine-status').inner_text())
                        raise
                    finally:
                        for route in held:
                            route.abort('connectionfailed')
                        context.close()
                assert all(item['method'] == 'GET' and not item['body_bytes'] and item['url'].startswith(base)
                           for item in result['requests']), 'Unexpected network destination or upload'
                runtime = [urlparse(item['url']) for item in result['requests']
                           if Path(urlparse(item['url']).path).suffix in {'.js', '.wasm', '.css'}]
                versions = {tuple(parse_qs(item.query).get('v', [])) for item in runtime}
                assert len(versions) == 1 and len(next(iter(versions))) == 1, versions
                result['runtime_version'] = next(iter(versions))[0]
                result['success'] = True
            finally:
                browser.close()
    except Exception as error:
        result['failure'] = str(error)
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
        (args.out / 'bootstrap-verification.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({key: value for key, value in result.items() if key != 'requests'}, indent=2))
    if not result['success']:
        raise SystemExit(1)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--site', type=Path, default=ROOT / 'web/dist')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    run(args)
