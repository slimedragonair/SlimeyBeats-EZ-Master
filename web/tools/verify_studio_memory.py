"""Bounded Studio replacement soak and actual WASM import limits; no forced GC."""
import argparse
import functools
import http.server
import json
from pathlib import Path
import threading
import time

from playwright.sync_api import sync_playwright, expect
from browser_capture import BrowserDiagnostics
from browser_memory import BrowserMemory
from verify_studio_browser import fixture, Handler, ROOT


def run(args):
    args.out.mkdir(parents=True, exist_ok=True)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    result = {'errors': [], 'cycles': [], 'forced_gc': False, 'seconds_per_source': args.seconds,
              'scope': 'One Studio workspace replaced without Close; cleanup, undo/redo and downloaded export per cycle. Not a device maximum.'}
    try:
        with sync_playwright() as p:
            browser = p.chromium.launch(headless=True, **({'executable_path': args.executable} if args.executable else {}))
            result['browser'] = browser.version
            memory = BrowserMemory(browser)
            page = browser.new_page(viewport={'width': 1480, 'height': 1000})
            diagnostics = BrowserDiagnostics(page, 'chromium', result)
            try:
                page.goto(f'http://127.0.0.1:{server.server_address[1]}/', wait_until='networkidle')
                expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
                page.locator('#tab-studio-tools').click()
                memory.refresh()
                result['initial_private_commit_MiB'] = memory.current()
                for cycle in range(args.tracks):
                    start = time.monotonic()
                    page.locator('#s-file').set_input_files(fixture(f'studio-{cycle}.wav', seconds=args.seconds))
                    expect(page.locator('#s-export')).to_be_enabled(timeout=120000)
                    page.locator('#s-tab-voice').click()
                    page.locator('#s-voice').click()
                    expect(page.locator('#s-export')).to_be_enabled(timeout=120000)
                    for id in ['undo', 'redo']:
                        page.locator('#s-' + id).click()
                        expect(page.locator('#s-export')).to_be_enabled(timeout=120000)
                    with page.expect_download(timeout=120000) as download:
                        page.locator('#s-export').click()
                    download.value.save_as(str(args.out / 'latest-preview.wav'))
                    download.value.delete()
                    expect(page.locator('#s-export')).to_be_enabled(timeout=30000)
                    worker = next(w for w in page.workers if '/studio-worker.js' in w.url)
                    heap = worker.evaluate("""async () => {
                        const url = new URL('./pkg/dsp.js', self.location.href); url.search = new URL(self.location.href).search;
                        const sdk = await import(url.href), wasm = await sdk.default(); return wasm.memory.buffer.byteLength / 1048576;
                    }""")
                    memory.refresh()
                    entry = {'cycle': cycle + 1, 'seconds': time.monotonic() - start, 'wasm_capacity_MiB': heap, 'private_commit_MiB': memory.current()}
                    result['cycles'].append(entry)
                    print(json.dumps(entry), flush=True)
                    if entry['private_commit_MiB'] and entry['private_commit_MiB'] > args.stop_mib:
                        raise RuntimeError('Test safety budget reached; not a browser limit measurement')
                if args.check_limits:
                    # In a separate worker-owned Studio, stage real PCM up to the
                    # combined budget, reject one frame over, then test duration.
                    result['limits'] = worker.evaluate("""async () => {
                        const url = new URL('./pkg/dsp.js', self.location.href); url.search = new URL(self.location.href).search;
                        const {BrowserStudio} = await import(url.href), studio = new BrowserStudio();
                        let overBytes = false, overTime = false;
                        try {
                            const samples = new Float32Array(16 * 1024 * 1024);
                            studio.stage_stem('vocals', samples, 48000, 2);
                            studio.stage_stem('instrumental', samples, 48000, 2);
                            try { studio.stage_stem('other', new Float32Array(2), 48000, 2); } catch(e) { overBytes = String(e).includes('128 MiB'); }
                        } finally { studio.free(); }
                        const duration = new BrowserStudio();
                        try {
                            duration.load(new Float32Array(8000 * 900), 8000, 1);
                            try { duration.load(new Float32Array(8000 * 900 + 1), 8000, 1); } catch(e) { overTime = String(e).includes('15 minutes'); }
                            const retained = JSON.parse(duration.info());
                            if (!overBytes || !overTime || retained.duration_seconds !== 900) throw new Error('Import boundary or source preservation failed');
                            return {combined_128_MiB_accepted: true, one_frame_over_combined_rejected: overBytes, seconds_900_at_8k_mono_accepted: true, one_frame_over_duration_rejected: overTime};
                        } finally { duration.free(); }
                    }""")
                page.locator('#s-close').click()
                page.wait_for_timeout(500)
                memory.refresh()
                result['after_close_private_commit_MiB'] = memory.current()
                assert not result['errors'], result['errors']
                result['passed'] = True
            except Exception as error:
                result['failure'] = str(error)
                diagnostics.capture(args.out / 'failure.png', full_page=True)
                raise
            finally:
                memory.close()
                result['peak_private_commit_MiB'] = memory.peak / 1048576 if memory.enabled else None
                (args.out / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
                browser.close()
    finally:
        server.shutdown()
    print(json.dumps({key: value for key, value in result.items() if key != 'cycles'}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--site', type=Path, default=ROOT / 'web/dist')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--executable')
    parser.add_argument('--tracks', type=int, default=12)
    parser.add_argument('--seconds', type=int, default=10)
    parser.add_argument('--stop-mib', type=int, default=3072)
    parser.add_argument('--check-limits', action='store_true')
    args = parser.parse_args()
    assert 0 < args.tracks <= 1000 and 0 < args.seconds <= 900
    run(args)
