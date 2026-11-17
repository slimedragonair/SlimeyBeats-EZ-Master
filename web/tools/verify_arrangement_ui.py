"""DOM/gesture qualification with mocked replies; does NOT qualify audio playback.

The unavailable-adapter check exercises the real controller's exposure gate.
Actual sound must pass the separate real arrangement-adapter browser workflow.
"""
import argparse
import functools
import http.server
import json
from pathlib import Path
import threading
import traceback
from urllib.parse import urlparse

from playwright.sync_api import expect, sync_playwright
from browser_capture import BrowserDiagnostics

ROOT = Path(__file__).resolve().parents[2]
HTML = b'''<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'self';script-src 'self';style-src 'self';img-src 'self' data:;connect-src 'self'">
<link rel="stylesheet" href="/styles.css"><link rel="stylesheet" href="/tools/arrangement-probe.css"></head>
<body><main id="arrangement-probe"></main><script type="module" src="/tools/arrangement_ui_probe.js"></script></body></html>'''
CSS = b'body{margin:0;min-width:0}#arrangement-probe{box-sizing:border-box;padding:12px;width:100%;max-width:1100px;margin:auto}'


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_unused):
        pass

    def translate_path(self, path):
        if urlparse(path).path == '/tools/arrangement_ui_probe.js':
            return str(ROOT / 'web/tools/arrangement_ui_probe.js')
        return super().translate_path(path)

    def do_GET(self):
        path = urlparse(self.path).path
        payload = HTML if path == '/tools/arrangement-ui.html' else CSS if path == '/tools/arrangement-probe.css' else None
        if payload is None:
            return super().do_GET()
        self.send_response(200)
        self.send_header('Content-Type', 'text/html' if path.endswith('.html') else 'text/css')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


def verify(browser, base, width, args, result):
    page = browser.new_page(viewport={'width': width, 'height': 844})
    row = {'width': width, 'errors': [], 'checks': []}
    result['cases'].append(row)
    diagnostics = BrowserDiagnostics(page, args.browser, row, interface_profile='fresh')
    try:
        page.goto(base + 'tools/arrangement-ui.html', wait_until='networkidle')
        page.wait_for_function('()=>!!window.arrangementUiProbe')
        gate = page.evaluate('()=>arrangementUiProbe.gate()')
        assert gate == {'ready': False, 'exposed': False, 'previewCalls': 0, 'installs': 0, 'edits': 0}, gate
        row['checks'].append('unavailable audio adapter leaves the real arranger hidden and dispatches no preview or edits')
        for mode in ['invalid-transport', 'stale']:
            rejected = page.evaluate('mode=>arrangementUiProbe.gate(mode)', mode)
            assert rejected == {'ready': False, 'exposed': False, 'previewCalls': 1, 'installs': 0, 'edits': 0}, rejected
        row['checks'].append('a playback flag alone cannot expose metadata-only or stale transports')
        page.evaluate('()=>arrangementUiProbe.mount()')
        wave = page.locator('[data-lane-id="lane-0"] .arr-lane-wave')
        expect(wave).to_be_visible()
        page.locator('[data-arr="lanes"]').scroll_into_view_if_needed()
        layout = page.evaluate('''()=>({client:document.documentElement.clientWidth,
            scroll:document.documentElement.scrollWidth,viewport:visualViewport.width,
            metrics:arrangementUiProbe.state().metrics})''')
        assert layout['scroll'] <= layout['client'] + 1 <= layout['viewport'] + 1, layout
        assert layout['metrics']['lanes'] < 16, layout
        row['checks'].append('1000-lane metadata keeps bounded mounted rows and no document overflow')
        clip = page.locator('[data-clip-id="clip-a"] .arr-clip-body').bounding_box()
        rect = wave.bounding_box()
        x, y = clip['x'] + clip['width'] / 2, clip['y'] + clip['height'] / 2
        page.mouse.move(x, y)
        page.mouse.down()
        page.mouse.move(x + rect['width'] * 0.1, y, steps=4)
        assert page.evaluate('()=>!!arrangementUiProbe.state().gesture')
        page.mouse.up()
        state = page.evaluate('()=>arrangementUiProbe.state()')
        assert state['unchanged'] and len(state['commands']) == 1, state
        assert state['commands'][0]['kind'] == 'move_clip' and state['commands'][0]['clip_id'] == 'clip-a'
        assert state['commands'][0]['start_frame'] > 48000
        row['checks'].append('real pointer move dispatches one typed edit on release and never mutates source metadata')
        page.evaluate('()=>arrangementUiProbe.clear()')
        destination = page.locator('[data-lane-id="lane-1"] .arr-lane-wave').bounding_box()
        page.evaluate('()=>{window.originalClipElement=document.querySelector("[data-clip-id=clip-a]")}')
        page.mouse.move(x, y)
        page.mouse.down()
        page.mouse.move(x + rect['width'] * 0.1, destination['y'] + destination['height'] / 2, steps=8)
        assert page.evaluate('()=>!originalClipElement.isConnected && !!arrangementUiProbe.state().gesture')
        page.mouse.up()
        state = page.evaluate('()=>arrangementUiProbe.state()')
        assert len(state['commands']) == 1 and state['commands'][0]['lane_id'] == 'lane-1' and state['unchanged'], state
        row['checks'].append('cross-track drag retains pointer capture while the original clip DOM is replaced')
        page.evaluate('()=>arrangementUiProbe.clear()')
        viewport = page.locator('[data-arr="lanes"]').bounding_box()
        page.mouse.move(x, y)
        page.mouse.down()
        page.mouse.move(min(width - 1, x + rect['width'] * 0.2), viewport['y'] + viewport['height'] + 20, steps=8)
        page.mouse.up()
        state = page.evaluate('()=>arrangementUiProbe.state()')
        assert len(state['commands']) == 1 and not state['gesture'] and state['unchanged'], state
        row['checks'].append('releasing outside the visible lane viewport commits exactly once')
        page.evaluate('()=>arrangementUiProbe.clear()')
        page.mouse.move(x, y)
        page.mouse.down()
        page.mouse.move(x + rect['width'] * 0.1, y, steps=4)
        page.keyboard.press('Escape')
        page.mouse.up()
        state = page.evaluate('()=>arrangementUiProbe.state()')
        assert not state['commands'] and not state['gesture'] and state['unchanged'], state
        row['checks'].append('Escape cancels gesture without dispatch or source changes')
        page.locator('[data-arr="clip-start"]').fill('1.125')
        page.locator('[data-arr="clip-end"]').fill('3.125')
        page.locator('[data-arr="clip-source"]').fill('0.5')
        assert page.locator('[data-arr="clip-start"]').evaluate('el=>el.validity.valid')
        page.locator('[data-arr="clip-timing"]').click()
        state = page.evaluate('()=>arrangementUiProbe.state()')
        assert state['commands'] == [[{'kind': 'trim_clip', 'clip_id': 'clip-a', 'start_frame': 54000,
                                      'source_start_frame': 24000, 'end_frame': 150000}]], state
        page.locator('[data-arr="clip-source"]').fill('')
        page.locator('[data-arr="clip-timing"]').click()
        assert len(page.evaluate('()=>arrangementUiProbe.state().commands')) == 1
        row['checks'].append('precise fractional timing emits exact frames; blank drafts dispatch nothing')
        page.locator('[data-arr="clip-source"]').fill('0.5')
        page.locator('[data-arr="clip-gain"]').fill('-2')
        page.evaluate('()=>arrangementUiProbe.gainReply(-2)')
        page.locator('[data-arr="clip-level"]').click()
        expect(page.locator('[data-arr="clip-start"]')).to_have_value('1.125')
        expect(page.locator('[data-arr="clip-end"]')).to_have_value('3.125')
        expect(page.locator('[data-arr="clip-gain"]')).to_have_value('-2')
        row['checks'].append('accepting gain/fades preserves the separate uncommitted timing draft')
        page.locator('[data-arr="clip-reload"]').click()
        gain = page.locator('[data-arr="clip-gain"]')
        gain.focus()
        page.evaluate('()=>arrangementUiProbe.receiveGain(-4)')
        expect(gain).to_have_value('-2')
        gain.fill('-3')
        before_conflict = len(page.evaluate('()=>arrangementUiProbe.state().commands'))
        page.locator('[data-arr="clip-level"]').click()
        assert len(page.evaluate('()=>arrangementUiProbe.state().commands')) == before_conflict
        expect(page.locator('[data-arr="clip-error"]')).to_contain_text('draft is kept')
        row['checks'].append('a focused field retains its value across remote metadata and refuses silent rebase')
        page.evaluate('()=>arrangementUiProbe.singleFrameSelection()')
        page.locator('[data-arr="fit-selection"]').click()
        state = page.evaluate('()=>arrangementUiProbe.state()')
        assert state['right'] - state['left'] == 480 and state['left'] <= 48000 < state['right'], state
        row['checks'].append('Fit selection makes a one-frame selection visible within a 10 ms view')
        page.locator('[data-arr="lanes"]').evaluate('el=>{el.scrollTop=132*450}')
        page.wait_for_function('()=>!!document.querySelector("[data-lane-id=lane-450]")')
        metrics = page.evaluate('()=>arrangementUiProbe.state().metrics')
        assert metrics['lanes'] < 16, metrics
        page.evaluate('()=>arrangementUiProbe.resetView()')
        page.locator('[data-arr="lanes"]').scroll_into_view_if_needed()
        diagnostics.capture(args.out / f'{width}-lanes.png')
        page.evaluate('()=>arrangementUiProbe.importReview()')
        checkbox = page.locator('[data-arr="import-parts"] input[type=checkbox]').first
        checkbox.focus()
        checkbox.press('Space')
        expect(checkbox).not_to_be_checked()
        assert checkbox.evaluate('el=>document.activeElement===el')
        checkbox.press('Space')
        expect(checkbox).to_be_checked()
        version = page.locator('[data-arr="import-parts"] select').first
        version.select_option('mp3-a')
        page.locator('[data-arr="import-parts"] input[type=checkbox]').nth(1).click()
        expect(version).to_have_value('mp3-a')
        row['checks'].append('part review preserves native checkbox focus and explicit encoding while other parts change')
        assert not row['errors'], row
        row.update(passed=True, layout=layout, final_metrics=metrics)
    except Exception as error:
        row.update(passed=False, failure=str(error), traceback=traceback.format_exc())
        diagnostics.capture(args.out / f'{width}-failure.png')
        raise
    finally:
        page.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--site', type=Path, default=ROOT / 'web/site')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    result = {'scope': 'DOM/gesture only, mocked replies; no audio qualification', 'cases': []}
    try:
        with sync_playwright() as p:
            browser = getattr(p, args.browser).launch(headless=True)
            try:
                result['browser'] = f'{args.browser} {browser.version}'
                for width in [320, 390, 1100]:
                    verify(browser, f'http://127.0.0.1:{server.server_address[1]}/', width, args, result)
            finally:
                browser.close()
        result['passed'] = True
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
        (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
        print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
