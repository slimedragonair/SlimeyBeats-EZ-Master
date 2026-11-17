"""Actual generated-WASM worker/player/worklet qualification with synthetic audio.

This tests browser PCM and control behavior, not physical-device listening.
"""
import argparse
import array
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
    parser.add_argument('--capture-start-delay-ms', type=int, default=0,
                        help='Qualification fault: delay real capture availability without changing product PCM')
    parser.add_argument('--capture-suppress-pcm', action='store_true',
                        help='Qualification negative control: worklet runs but transfers no PCM')
    parser.add_argument('--pcm-timeout-ms', type=int, default=10000)
    args = parser.parse_args()
    if not 0 <= args.capture_start_delay_ms <= 30000 or not 100 <= args.pcm_timeout_ms <= 30000:
        parser.error('Capture delay must be 0..30000 ms; PCM deadline must be 100..30000 ms')
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
                probe_options = {'captureDelayMs': args.capture_start_delay_ms,
                                 'suppressPcm': args.capture_suppress_pcm,
                                 'pcmTimeoutMs': args.pcm_timeout_ms}
                result['probe_options'] = probe_options
                page.add_init_script('''(() => {
                    const settings = ''' + json.dumps(probe_options) + ''';
                    globalThis.arrangementAudioProbeOptions = settings;
                    const Original = globalThis.AudioWorkletNode;
                    globalThis.AudioWorkletNode = class extends Original {
                        constructor(context, name, options) {
                            if (name === "arrangement-capture") options = { ...options,
                                processorOptions: { ...options?.processorOptions,
                                    captureDelayFrames: Math.ceil(settings.captureDelayMs * context.sampleRate / 1000),
                                    suppressPcm: settings.suppressPcm } };
                            super(context, name, options);
                        }
                    };
                })();''')
                page.goto(f'http://127.0.0.1:{server.server_address[1]}/_arrangement-test.html')
                try:
                    probe = Path(__file__).with_name('arrangement_audio_probe.js').read_text(encoding='utf-8')
                    result.update(page.evaluate('''async () => {
                        let timer;
                        try { return await Promise.race([(''' + probe + ''')(),
                            new Promise((_, reject) => { timer = setTimeout(() => reject(new Error("Arrangement audio probe exceeded 120 s")), 120000); })]); }
                        finally { clearTimeout(timer); }
                    }'''))
                    assert not errors, errors
                finally:
                    result['page_errors'] = errors
                    try:
                        result['diagnostics'] = page.evaluate('() => globalThis.arrangementAudioProbeDiagnostics || null')
                        for index, window in enumerate((result['diagnostics'] or {}).get('waits', [])):
                            samples = window.pop('raw_pcm', None)
                            if samples is not None:
                                pcm = array.array('f', samples)
                                if __import__('sys').byteorder != 'little':
                                    pcm.byteswap()
                                filename = f'pcm-{index + 1:02d}-{window["label"]}.f32'
                                (args.out / filename).write_bytes(pcm.tobytes())
                                window['raw_pcm'] = {'file': filename, 'format': 'f32le', 'channels': 2, 'samples': len(pcm)}
                    except Exception as diagnostic_error:
                        result['diagnostics_error'] = str(diagnostic_error)
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
