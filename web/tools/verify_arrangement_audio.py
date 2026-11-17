"""Actual generated-WASM worker/player/worklet qualification with synthetic audio.

This tests browser PCM and control behavior, not physical-device listening.
"""
import argparse
import functools
import http.server
import json
from pathlib import Path
import shutil
import threading
import traceback
import io
import wave
import zipfile

from playwright.sync_api import sync_playwright


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_args):
        pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--site', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    (args.site / '_arrangement-test.html').write_text('<!doctype html><meta charset="utf-8"><title>Arrangement adapter qualification</title>', encoding='utf-8')
    shutil.copy2(Path(__file__).with_name('arrangement_audio_capture.js'), args.site / 'arrangement_audio_capture.js')
    audio = io.BytesIO()
    with wave.open(audio, 'wb') as writer:
        writer.setparams((1, 2, 48000, 0, 'NONE', 'not compressed'))
        writer.writeframes(b'\0\0' * 4800)
    for name, entry in [('single-source.zip', 'wav/Only.wav'), ('unsafe-source.zip', '../escape.wav')]:
        with zipfile.ZipFile(args.site / name, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr(entry, audio.getvalue())
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    result = {'scope': 'real generated WASM and AudioWorklet PCM; no physical-device acceptance'}
    try:
        with sync_playwright() as p:
            options = {'args': ['--autoplay-policy=no-user-gesture-required']} if args.browser == 'chromium' else {}
            browser = getattr(p, args.browser).launch(headless=True, **options)
            try:
                result['browser'] = f'{args.browser} {browser.version}'
                page = browser.new_page()
                errors = []
                page.on('pageerror', lambda error: errors.append(str(error)))
                page.goto(f'http://127.0.0.1:{server.server_address[1]}/_arrangement-test.html')
                result.update(page.evaluate(Path(__file__).with_name('arrangement_audio_probe.js').read_text(encoding='utf-8')))
                result['page_errors'] = errors
                assert not errors, errors
            finally:
                browser.close()
    except Exception as error:
        result.update(passed=False, error=str(error), traceback=traceback.format_exc())
        raise
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
        (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
        print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
