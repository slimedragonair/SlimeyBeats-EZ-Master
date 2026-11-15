"""Exercise actual mastering/Studio MP3 and portable downloads, without uploads."""
import argparse
import functools
import http.server
import io
import json
import math
from pathlib import Path
import struct
import subprocess
import threading
from urllib.parse import urlparse
import wave
import zipfile

from playwright.sync_api import sync_playwright, expect
from browser_capture import BrowserDiagnostics

ROOT = Path(__file__).resolve().parents[2]


def fixture(name, frequency):
    data = io.BytesIO()
    with wave.open(data, 'wb') as writer:
        writer.setparams((2, 2, 48000, 0, 'NONE', 'not compressed'))
        samples = bytearray()
        for frame in range(48000 * 4):
            fade = min(1, frame / 2400, (192000 - frame) / 2400)
            for offset in [0, 73]:
                value = .23 * fade * math.sin(2 * math.pi * (frequency + offset) * frame / 48000)
                samples.extend(struct.pack('<h', round(value * 32767)))
        writer.writeframes(samples)
    return {'name': name, 'mimeType': 'audio/wav', 'buffer': data.getvalue()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--site', type=Path, default=ROOT / 'web/dist')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    class Handler(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *unused):
            pass

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    result = {'checks': [], 'requests': [], 'errors': []}
    try:
        with sync_playwright() as engine:
            browser = getattr(engine, args.browser).launch(headless=True)
            result['browser'] = args.browser + ' ' + browser.version
            page = browser.new_page(viewport={'width': 1480, 'height': 1000})
            diagnostics = BrowserDiagnostics(page, args.browser, result)
            page.on('request', lambda r: result['requests'].append({'method': r.method, 'host': urlparse(r.url).hostname, 'body_bytes': len(r.post_data_buffer or b'')}))
            try:
                page.goto(f'http://127.0.0.1:{server.server_address[1]}/', wait_until='networkidle')
                expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)

                def save(selector, name):
                    with page.expect_download(timeout=120000) as pending:
                        page.locator(selector).click()
                    target = args.out / name
                    pending.value.save_as(str(target))
                    return target.read_bytes()

                def master_ready():
                    expect(page.locator('#export')).to_be_enabled(timeout=120000)

                def studio_ready():
                    expect(page.locator('#s-export')).to_be_enabled(timeout=120000)

                page.locator('#file').set_input_files(fixture('Export validation.wav', 440))
                expect(page.locator('#master')).to_be_enabled(timeout=30000)
                page.locator('#master').click()
                master_ready()
                page.locator('#format').select_option('mp3')
                expect(page.locator('#export')).to_contain_text('MP3')
                expect(page.locator('#dither')).to_be_disabled()
                first = save('#export', 'master.mp3')
                master_ready()
                page.locator('#playback-details > summary').click()
                page.locator('#volume').fill('0.1')
                page.locator('#match').uncheck()
                assert save('#export', 'master-listener-volume.mp3') == first
                master_ready()
                result['checks'].append('master MP3 uses current render; listener volume/matching leave encoded bytes unchanged')

                page.locator('#tab-report').click()
                eq = json.loads(save('#save-eq', 'preset.eq.json'))
                assert eq['version'] == 2 and len(eq['bands']) == 7
                report = json.loads(save('#save-report', 'master-report.json'))
                exported = report['exports'][0]
                assert exported['format'] == 'mp3' and exported['bitrate_kbps'] == 320
                assert exported['decoded_metrics']['true_peak_dbtp'] <= report['ceiling_dbtp'] + .005
                assert report['output']['metrics'] == exported['decoded_metrics']
                bundle = save('#save-session', 'master-session.zip')
                master_ready()
                with zipfile.ZipFile(io.BytesIO(bundle)) as archive:
                    assert archive.testzip() is None
                    assert set(archive.namelist()) == {'master.mp3', 'analysis.json', 'eq-profile.json', 'settings.json', 'session.json', 'manifest.json'}
                    assert archive.read('master.mp3') == first
                    assert json.loads(archive.read('eq-profile.json')) == eq
                    manifest = json.loads(archive.read('manifest.json'))
                    assert manifest['original_audio_included'] is False
                    settings = json.loads(archive.read('settings.json'))
                    assert settings['formats'] == ['mp3']
                result['checks'].append('desktop-compatible EQ preset and MP3 session bundle agree with the verified download; valid ZIP CRC and no original')
                diagnostics.capture(args.out / 'mastering-session-downloads.png')
                page.locator('#profile').select_option('edm')
                expect(page.locator('#save-session')).to_be_disabled()
                page.locator('#profile').select_option('neutral')
                master_ready()
                page.locator('#format').select_option('wav24')
                wav_bundle = save('#save-session', 'master-wav-session.zip')
                master_ready()
                with zipfile.ZipFile(io.BytesIO(wav_bundle)) as archive:
                    assert 'master.wav' in archive.namelist()
                    with wave.open(io.BytesIO(archive.read('master.wav'))) as audio:
                        assert audio.getsampwidth() == 3 and audio.getnframes() == 192000
                result['checks'].append('session export follows chosen WAV/MP3 format and rejects stale automatic direction')
                for width in [320, 390]:
                    page.set_viewport_size({'width': width, 'height': 844})
                    assert page.evaluate('() => document.documentElement.scrollWidth <= document.documentElement.clientWidth')
                    for selector in ['#save-report', '#save-eq', '#save-session']:
                        page.locator(selector).evaluate('el => el.scrollIntoView({block:"center"})')
                        assert page.locator(selector).evaluate('el => { const r=el.getBoundingClientRect(); return el.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)); }'), selector
                page.set_viewport_size({'width': 1480, 'height': 1000})
                result['checks'].append('report, EQ and session downloads fit and receive ordinary pointer hits at 320/390px')

                # Hold a real packaging result to reproduce edits/cancel after
                # rendering but before publication, without a timing guess.
                downloads = []
                page.on('download', lambda value: downloads.append(value.suggested_filename))
                page.evaluate('''() => {
                    window.ExportNativeWorker = window.Worker;
                    window.Worker = class extends window.ExportNativeWorker {
                        constructor(url, options) {
                            super(url, options);
                            if (String(url).includes('session-export-worker.js'))
                                Object.defineProperty(this, 'onmessage', { set: handler => {
                                    this.addEventListener('message', event => {
                                        window.releasePackage = () => handler(event);
                                    });
                                }});
                        }
                    };
                }''')
                page.locator('#save-session').click()
                page.wait_for_function('() => typeof window.releasePackage === "function"')
                page.locator('#tab-eq').click()
                page.locator('#bands input[data-band="2"][data-parameter="gain"]').fill('3')
                page.evaluate('() => { window.releasePackage(); delete window.releasePackage; }')
                expect(page.locator('#status')).to_contain_text('Settings changed during export', timeout=30000)
                master_ready()
                assert not downloads
                page.locator('#tab-report').click()
                page.locator('#save-session').click()
                page.wait_for_function('() => typeof window.releasePackage === "function"')
                page.locator('#cancel').click()
                expect(page.locator('#status')).to_contain_text('Cancelled. Your original is ready', timeout=30000)
                page.evaluate('() => { window.releasePackage(); delete window.releasePackage; window.Worker = window.ExportNativeWorker; }')
                expect(page.locator('#export')).to_be_disabled()
                expect(page.locator('#filename')).to_have_text('Export validation.wav')
                assert not downloads
                result['checks'].append('late ZIP results cannot save stale EQ or publish after cancellation; original source retained')

                page.locator('#tab-studio-tools').click()
                page.locator('#s-bulk-files').set_input_files([fixture('Vocals.wav', 330), fixture('Drums.wav', 110)])
                expect(page.locator('#s-import-batch')).to_be_enabled(timeout=30000)
                page.locator('#s-import-batch').click()
                studio_ready()
                page.locator('#s-format').select_option('mp3')
                studio_mp3 = save('#s-export', 'studio.mp3')
                studio_ready()
                bundle = save('#s-export-session', 'studio-session.zip')
                studio_ready()
                with zipfile.ZipFile(io.BytesIO(bundle)) as archive:
                    assert archive.testzip() is None
                    assert archive.read('mix.mp3') == studio_mp3
                    stems = json.loads(archive.read('stems.json'))
                    assert len(stems['stems']) == 2
                    assert all(stem['file'] in archive.namelist() for stem in stems['stems'])
                    session = json.loads(archive.read('session.json'))
                    assert session['original_audio_included'] is False
                page.locator('#s-faders input[type=range]').first.fill('-6')
                expect(page.locator('#s-export-session')).to_be_disabled()
                page.locator('#s-mix').click()
                studio_ready()
                expect(page.locator('#s-export-session')).to_be_enabled()
                result['checks'].append('Studio MP3 and complete aligned stem bundle; pending live mix blocks session save until committed')
                diagnostics.capture(args.out / 'studio-desktop-exports.png')

                for width in [390, 320]:
                    page.set_viewport_size({'width': width, 'height': 844})
                    assert page.evaluate('() => document.documentElement.scrollWidth <= document.documentElement.clientWidth')
                    page.locator('#s-export-session').scroll_into_view_if_needed()
                    expect(page.locator('#s-export-session')).to_be_visible()
                    diagnostics.capture(args.out / f'studio-exports-{width}.png', full_page=True)
                result['checks'].append('session/MP3 controls fit at 320px and 390px')
                for name in ['master.mp3', 'studio.mp3']:
                    probe = subprocess.run(['ffprobe', '-v', 'error', '-show_streams', '-of', 'json', str(args.out / name)], capture_output=True, text=True, check=True)
                    stream = json.loads(probe.stdout)['streams'][0]
                    assert stream['codec_name'] == 'mp3' and int(stream['sample_rate']) == 48000 and stream['channels'] == 2
                    assert 4 <= float(stream['duration']) <= 4.25
                    check = subprocess.run(['ffmpeg', '-hide_banner', '-nostats', '-i', str(args.out / name), '-filter_complex', 'ebur128=peak=true', '-f', 'null', '-'], capture_output=True, text=True, check=True)
                    (args.out / (name + '.ffmpeg.txt')).write_text(check.stderr, encoding='utf-8')
                assert all(r['method'] == 'GET' and r['host'] == '127.0.0.1' and r['body_bytes'] == 0 for r in result['requests'])
                assert not result['errors'], result['errors']
                result['passed'] = True
            except Exception as error:
                result.update(passed=False, failure=str(error), status=page.locator('#status').inner_text(), studio_status=page.locator('#s-status').inner_text())
                diagnostics.capture(args.out / 'failure.png', full_page=True)
            finally:
                browser.close()
    finally:
        server.shutdown(); server.server_close(); thread.join()
        (args.out / 'export-workflows.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({k: v for k, v in result.items() if k != 'requests'}, indent=2))
    if not result.get('passed'):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
