"""Actual browser reload/recovery/report privacy checks, against a built static site."""
import argparse
import functools
import http.server
import json
from pathlib import Path
import threading
from urllib.parse import urlparse
from playwright.sync_api import sync_playwright, expect
from verify_studio_browser import fixture
from recovery_studio_checks import studio_recovery, import_race, chromium_crash

ROOT = Path(__file__).resolve().parents[2]


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def translate_path(self, path):
        if urlparse(path).path == '/tools/recovery_store_checks.js':
            return str(ROOT / 'web/tools/recovery_store_checks.js')
        return super().translate_path(path)


def point_count(page):
    return page.evaluate("async () => (await (new (await import('/recovery-store.js')).RecoveryStore()).list()).length")


def open_recovery(page):
    page.evaluate("document.dispatchEvent(new Event('open-recovery'))")
    expect(page.locator('#recovery-dialog')).to_be_visible()


def ready(page):
    expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)


def save_point(page):
    page.locator('#recovery-save').click()
    expect(page.locator('#recovery-status')).to_contain_text('Recovery saved locally', timeout=30000)


def audio_export(page, out):
    with page.expect_download(timeout=120000) as event:
        page.locator('#export').click()
    event.value.save_as(str(out))
    expect(page.locator('#export')).to_be_enabled(timeout=120000)
    return out.read_bytes()


def run(args):
    args.out.mkdir(parents=True, exist_ok=True)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    base = f'http://127.0.0.1:{server.server_address[1]}/'
    result = {'passed': False, 'checks': [], 'requests': [], 'errors': []}
    try:
        with sync_playwright() as pw:
            browser = getattr(pw, args.browser).launch(headless=True)
            result['browser'] = args.browser + ' ' + browser.version
            context = browser.new_context(viewport={'width': 1480, 'height': 1000}, accept_downloads=True)
            context.add_init_script("localStorage.setItem('slimeybeats.interface-preferences.tutorial-dismissed','declined')")
            page = context.new_page()
            page.on('dialog', lambda d: d.accept())
            page.on('pageerror', lambda error: result['errors'].append({'message': str(error), 'stack': error.stack, 'after_check': result['checks'][-1] if result['checks'] else 'initial storage checks'}))
            context.on('request', lambda r: result['requests'].append({'method': r.method, 'url': r.url, 'body': len(r.post_data_buffer or b'')}))
            page.goto(base, wait_until='networkidle'); ready(page)
            try:
                result['checks'] += page.evaluate("async () => (await import('/tools/recovery_store_checks.js')).checkRecoveryStore()")
            except Exception:
                result['store_step'] = page.evaluate('window.recoveryTestStep')
                print('Recovery store step:', result['store_step'])
                raise
            page.locator('#file').set_input_files(fixture('PRIVATE_SONG_CANARY.wav', seconds=.6))
            expect(page.locator('#master')).to_be_enabled(timeout=30000)
            page.locator('#master').click()
            expect(page.locator('#export')).to_be_enabled(timeout=120000)
            page.wait_for_timeout(1800)
            assert point_count(page) == 0, 'Default-off session stored audio'
            assert not page.locator('#recovery-offer').is_visible()
            open_recovery(page)
            with page.expect_download() as event:
                page.locator('#recovery-download').click()
            event.value.save_as(str(args.out / 'manual-off.sbrecovery'))
            assert (args.out / 'manual-off.sbrecovery').stat().st_size > 0
            page.locator('#recovery-enable').click()
            expect(page.locator('#recovery-enabled')).to_have_text('Enabled on this device')
            save_point(page)
            page.locator('#recovery-close').click()
            page.locator('#tab-eq').click()
            page.locator('[data-band="2"][data-parameter="gain"]').fill('-2')
            page.locator('[data-band="2"][data-parameter="gain"]').dispatch_event('input')
            baseline = audio_export(page, args.out / 'before-reload.wav')
            open_recovery(page); save_point(page)
            page.screenshot(path=str(args.out / 'recovery-desktop.png'))
            page.reload(wait_until='networkidle'); ready(page)
            expect(page.locator('#recovery-offer')).to_be_visible()
            expect(page.locator('#track-card')).to_be_hidden()
            page.locator('#recovery-offer-open').click()
            page.locator('#recovery-points button').first.click()
            expect(page.locator('#recovery-action-status')).to_contain_text('Restored with playback stopped', timeout=120000)
            page.locator('#recovery-close').click()
            expect(page.locator('#play')).to_have_text('Play')
            expect(page.locator('#export')).to_be_enabled()
            assert audio_export(page, args.out / 'after-reload.wav') == baseline, 'Restored export differs'
            page.locator('#tab-eq').click()
            expect(page.locator('[data-band="2"][data-parameter="gain"]')).to_have_value('-2')
            result['checks'].append('default-off manual backup, opt-in actual PCM/EQ/master reload, exact export, no playback/automatic restore')
            # A real UI save failure must be visible outside a closed Help panel.
            page.evaluate('''async () => {
                const resource = performance.getEntriesByType('resource').find(e => e.name.includes('/recovery-store.js?v=')).name;
                const Store = (await import(resource)).RecoveryStore, save = Store.prototype.save;
                Store.prototype.save = function() { Store.prototype.save = save; return Promise.reject(new DOMException('private context','QuotaExceededError')); };
            }''')
            open_recovery(page); page.locator('#recovery-save').click()
            expect(page.locator('#recovery-status')).to_contain_text('storage is full')
            page.locator('#recovery-close').click()
            page.set_viewport_size({'width': 320, 'height': 568})
            expect(page.locator('#recovery-paused')).to_be_visible()
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth + 1')
            page.locator('#recovery-paused').click()
            expect(page.locator('#recovery-status')).to_contain_text('storage is full')
            page.locator('#recovery-disable').click(); page.locator('#recovery-enable').click()
            expect(page.locator('#recovery-paused')).to_be_hidden()
            page.locator('#recovery-close').click(); page.set_viewport_size({'width': 1480, 'height': 1000})
            result['checks'].append('quota failure exposes a reachable Save paused indicator at 320px without disabling audio editing')
            open_recovery(page); save_point(page)
            page.locator('#recovery-points button').first.click()
            expect(page.locator('#recovery-action-status')).to_contain_text('Current work is kept')
            page.locator('#recovery-close').click()
            other = context.new_page(); other.goto(base, wait_until='networkidle'); ready(other)
            open_recovery(other)
            other.locator('#recovery-points button').first.click()
            expect(other.locator('#recovery-action-status')).to_contain_text('open in another tab')
            other.locator('#recovery-disable').click()
            open_recovery(page)
            expect(page.locator('#recovery-enabled')).to_have_text('Off by default')
            count = point_count(page)
            page.locator('#recovery-close').click()
            page.locator('#profile').select_option('edm'); page.wait_for_timeout(1800)
            assert point_count(page) == count
            page.locator('#profile').select_option('neutral')
            other.locator('#recovery-delete').click(); other.locator('#recovery-delete-now').click()
            expect(other.locator('#recovery-points')).to_contain_text('No recovery points')
            expect(page.locator('#export')).to_be_enabled()
            other.close()
            result['checks'].append('active tab lock respected, current work preserved, cross-tab disable/delete cannot resurrect points')
            page.evaluate("document.dispatchEvent(new Event('open-support-report'))")
            expect(page.locator('#support-copy')).to_be_disabled()
            page.locator('#support-description').fill('File at C:\\Users\\PRIVATE_PERSON\\song.wav\n/home/PRIVATE_PERSON/music.wav\nPlease contact private@example.com')
            page.locator('#support-build').click()
            preview = page.locator('#support-preview').inner_text()
            for canary in ['PRIVATE_SONG_CANARY', 'PRIVATE_PERSON', 'private@example.com']:
                assert canary not in preview
            page.evaluate("() => { navigator.clipboard.writeText = async value => { window.reportClipboardFixture = value; }; }")
            page.locator('#support-copy').click()
            assert page.evaluate('window.reportClipboardFixture') == preview
            page.evaluate("() => { navigator.clipboard.writeText = async () => { throw new DOMException('blocked','NotAllowedError'); }; }")
            page.locator('#support-copy').click()
            expect(page.locator('#support-status')).to_contain_text('Clipboard access is blocked')
            with page.expect_download() as event:
                page.locator('#support-download').click()
            event.value.save_as(str(args.out / 'reviewed-report.txt'))
            assert (args.out / 'reviewed-report.txt').read_text(encoding='utf-8') == preview
            page.set_viewport_size({'width': 390, 'height': 844})
            page.screenshot(path=str(args.out / 'report-phone.png'))
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth + 1')
            page.locator('#support-description').fill('New description')
            expect(page.locator('#support-copy')).to_be_disabled()
            expect(page.locator('#support-download')).to_be_disabled()
            page.locator('#support-close').click()
            result['checks'].append('report preview/download byte agreement, copy API/blocked-clipboard fallback, edit invalidates approval, privacy canaries and 390px layout')
            # A fresh, blocked-storage context still has manual recovery and reporting.
            blocked = browser.new_context()
            blocked.add_init_script("Object.defineProperty(window,'indexedDB',{get(){throw new DOMException('blocked','SecurityError')}})")
            b = blocked.new_page(); b.goto(base, wait_until='networkidle'); ready(b)
            b.evaluate("document.dispatchEvent(new Event('open-recovery'))")
            expect(b.locator('#recovery-status')).to_contain_text('unavailable or blocked')
            b.locator('#recovery-close').click()
            b.locator('#file').set_input_files(fixture('blocked.wav', seconds=.2)); expect(b.locator('#master')).to_be_enabled()
            open_recovery(b)
            with b.expect_download() as event: b.locator('#recovery-download').click()
            assert event.value.failure() is None
            blocked.close()
            # Neither WASM nor guide delivery is required to create a support report.
            failed = browser.new_context()
            failed.add_init_script("localStorage.setItem('slimeybeats.interface-preferences.tutorial-dismissed','declined')")
            failed.route('**/*.wasm*', lambda route: route.abort())
            failed.route('**/learning-guide.json*', lambda route: route.abort())
            f = failed.new_page(); f.goto(base, wait_until='networkidle')
            expect(f.locator('#engine-status')).to_have_text('Engine unavailable', timeout=30000)
            f.locator('#engine-report').click(); f.locator('#support-build').click()
            expect(f.locator('#support-preview')).to_contain_text('startup_failed')
            failed.close()
            result['checks'].append('blocked IndexedDB keeps editing/manual backup usable; report works without WASM or guidance')
            result['checks'] += studio_recovery(browser, base, args.out, ready, open_recovery, save_point)
            result['checks'] += import_race(browser, base, args.out, ready, open_recovery)
            if args.browser == 'chromium':
                result['checks'] += chromium_crash(browser, base, ready, open_recovery, save_point)
            assert not result['errors'], result['errors']
            assert all(r['method'] == 'GET' and r['body'] == 0 and r['url'].startswith(base) for r in result['requests'])
            result['checks'].append('all observed requests same-origin static GET; no audio or report uploads')
            result['passed'] = True
            browser.close()
    except Exception as error:
        result['failure'] = str(error)
        raise
    finally:
        server.shutdown(); (args.out / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps({'browser': result['browser'], 'checks': result['checks']}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--site', type=Path, default=ROOT / 'web/dist')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    run(parser.parse_args())
