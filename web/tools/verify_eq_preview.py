"""Compare production worklet PCM with shared WASM and measure warmed edit blocks.

Synthetic desktop timing is diagnostic, not a physical audio-device qualification.
The instrumented route adds deterministic control messages to the real processor.
"""
import argparse
import functools
import http.server
import json
from pathlib import Path
import threading
from urllib.parse import urlparse

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--site', type=Path, default=ROOT / 'web/dist')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    args = parser.parse_args()

    class Handler(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *unused):
            pass

        def do_GET(self):
            path = urlparse(self.path).path
            if path == '/tools/eq-preview.html':
                payload = b'<!doctype html><title>Local EQ preview probe</title><script type="module" src="/tools/eq_preview_probe.js"></script>'
                kind = 'text/html'
            elif path == '/tools/eq_preview_probe.js':
                payload = (ROOT / 'web/tools/eq_preview_probe.js').read_bytes()
                kind = 'text/javascript'
            elif path == '/audio/eq-preview-instrumented.js':
                production = (args.site / 'audio/eq-processor.js').read_text(encoding='utf-8')
                registration = 'registerProcessor("slimey-eq", SlimeyEq);'
                if production.count(registration) != 1:
                    self.send_error(500, 'Production EQ registration changed; review probe')
                    return
                wrapper = '''
class EqPreviewProbe extends SlimeyEq {
    constructor(options) {
        super(options);
        this.probeEdits = options.processorOptions.edits;
        this.probeFrame = 0;
        this.probeApplied = 0;
        const apply = this.eq.set_settings.bind(this.eq);
        this.eq.set_settings = (...args) => {
            this.probeApplied++;
            return apply(...args);
        };
    }
    process(inputs, outputs) {
        for (const edit of this.probeEdits) {
            if (edit.frame === this.probeFrame) {
                for (const settings of edit.settings)
                    this.port.onmessage({data: {settings, revision: edit.frame}});
            }
        }
        const result = super.process(inputs, outputs);
        if (this.probeEdits.some(edit => edit.frame === this.probeFrame))
            this.port.postMessage({probeApplied: this.probeApplied, probeFrame: this.probeFrame});
        this.probeFrame += outputs[0][0].length;
        return result;
    }
}
registerProcessor("slimey-eq-probe", EqPreviewProbe);
'''
                payload = production.replace(registration, wrapper).encode('utf-8')
                kind = 'text/javascript'
            elif path == '/audio/eq-preview-capture.js':
                payload = b'''class EqPreviewCapture extends AudioWorkletProcessor {
                    process(inputs, outputs) {
                        const channels = inputs[0]?.length ? inputs[0] : outputs[0];
                        this.port.postMessage({time: currentTime, channels: channels.map(c => c.slice())});
                        return true;
                    }
                }
                registerProcessor("eq-preview-capture", EqPreviewCapture);'''
                kind = 'text/javascript'
            else:
                return super().do_GET()
            self.send_response(200)
            self.send_header('Content-Type', kind)
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def end_headers(self):
            # Enables a useful diagnostic clock without changing the production site.
            self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
            self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
            super().end_headers()

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    result = {'errors': [], 'requests': [], 'timing_scope': 'synthetic desktop direct WASM blocks; no physical output device'}
    try:
        with sync_playwright() as p:
            browser = getattr(p, args.browser).launch(headless=True)
            result['browser'] = args.browser + ' ' + browser.version
            page = browser.new_page()
            page.on('pageerror', lambda error: result['errors'].append(str(error)))
            page.on('request', lambda request: result['requests'].append({'method': request.method, 'url': request.url, 'body_bytes': len(request.post_data_buffer or b'')}))
            try:
                page.goto(f'http://127.0.0.1:{server.server_address[1]}/tools/eq-preview.html')
                page.wait_for_function('()=>typeof window.runEqPreview === "function"')
                result['preview'] = page.evaluate('()=>window.runEqPreview()')
                assert not result['errors'], result['errors']
                assert all(r['method'] == 'GET' and not r['body_bytes'] and urlparse(r['url']).hostname == '127.0.0.1' for r in result['requests'])
                result['success'] = True
            except Exception as error:
                result['success'] = False
                result['failure'] = str(error)
            finally:
                browser.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({k: v for k, v in result.items() if k != 'requests'}, indent=2))
    if not result.get('success'):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
