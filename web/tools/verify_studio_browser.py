"""Exercise real Studio controls, shared WASM, failure recovery and downloads."""
import argparse
import array
import functools
import http.server
import io
import json
import math
from pathlib import Path
import threading
import wave
import zipfile

from playwright.sync_api import sync_playwright, expect
from browser_capture import BrowserDiagnostics
from studio_waveform_checks import verify_waveform
from studio_multitrack_checks import verify_multitrack
from studio_bundle_checks import verify_bundle
from studio_export_checks import audio_names, verify_manifest

ROOT = Path(__file__).resolve().parents[2]


def fixture(name, hz=440, seconds=3):
    pcm = array.array('h', (int(2200 * math.sin(i * hz * math.tau / 48000)) for i in range(int(48000 * seconds))))
    output = io.BytesIO()
    with wave.open(output, 'wb') as writer:
        writer.setparams((1, 2, 48000, 0, 'NONE', 'not compressed'))
        writer.writeframes(pcm.tobytes())
    return {'name': name, 'mimeType': 'audio/wav', 'buffer': output.getvalue()}


class TestServer(http.server.ThreadingHTTPServer):
    request_queue_size = 64


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *unused):
        pass


def run(args):
    args.out.mkdir(parents=True, exist_ok=True)
    server = TestServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    result = {'errors': [], 'requests': [], 'checks': []}
    try:
        with sync_playwright() as p:
            browser = getattr(p, args.browser).launch(headless=True, **({'executable_path': args.executable} if args.executable else {}))
            result['browser'] = args.browser + ' ' + browser.version
            page = browser.new_page(viewport={'width': 1480, 'height': 1000})
            diagnostics = BrowserDiagnostics(page, args.browser, result)
            page.on('request', lambda request: result['requests'].append({'method': request.method, 'url': request.url, 'body_bytes': len(request.post_data_buffer or b'')}))
            try:
                page.goto(f'http://127.0.0.1:{server.server_address[1]}/', wait_until='networkidle')
                expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
                page.locator('#tab-studio-tools').click()
                expect(page.locator('#studio-tools')).to_be_visible()
                expect(page.locator('.playback')).to_be_hidden()
                result['waveform'] = verify_waveform(page)
                page.locator('#s-file').set_input_files(fixture('voice.wav'))

                def ready():
                    expect(page.locator('#s-export')).to_be_enabled(timeout=120000)

                def save(id, name):
                    with page.expect_download(timeout=120000) as download:
                        page.locator(id).click()
                    path = args.out / name
                    download.value.save_as(str(path))
                    ready()
                    return path.read_bytes()

                ready()
                expect(page.locator('#analyze')).to_be_enabled()
                page.locator('#analyze').click()
                ready()
                expect(page.locator('#s-status')).to_contain_text('LUFS integrated')
                baseline = save('#s-export', 'source.wav')
                page.locator('#s-tab-voice').click()
                page.locator('#s-voice').click()
                ready()
                cleaned = save('#s-export', 'voice-cleaned.wav')
                assert cleaned != baseline
                page.locator('#s-undo').click()
                ready()
                assert save('#s-export', 'undo.wav') == baseline
                page.locator('#s-redo').click()
                ready()
                assert save('#s-export', 'redo.wav') == cleaned
                result['checks'].append('voice cleanup and exact undo/redo exports')

                # Invalid DSP parameters must preserve the committed preview.
                page.locator('#s-highpass').fill('5')
                page.locator('#s-voice').click()
                expect(page.locator('#s-status')).to_contain_text('20–300', timeout=30000)
                ready()
                assert save('#s-export', 'after-error.wav') == cleaned
                page.locator('#s-highpass').fill('75')
                # Same browser turn: cancellation precedes completion of the worker step.
                page.evaluate("() => { document.querySelector('#s-voice').click(); document.querySelector('#s-cancel').click(); }")
                ready()
                expect(page.locator('#s-status')).to_contain_text('Cancelled')
                assert save('#s-export', 'after-cancel.wav') == cleaned
                result['checks'].append('failed edit and cancelled edit preserve prior preview')

                page.locator('.edit-import summary').click()
                for kind, hz in [('vocals', 440), ('instrumental', 220)]:
                    page.locator(f'#s-stem-files input[data-kind="{kind}"]').set_input_files(fixture(f'{kind}.wav', hz))
                page.locator('#s-import').click()
                ready()
                raw_two = save('#s-export-stems', 'two-stems.zip')
                assert audio_names(zipfile.ZipFile(io.BytesIO(raw_two))) == {'vocals.wav', 'instrumental.wav'}
                verify_manifest(zipfile.ZipFile(io.BytesIO(raw_two)))
                page.locator('#s-tab-mix').click()
                page.get_by_role('slider', name='vocals Gain / dB', exact=True).fill('-6')
                page.locator('#s-mix').click()
                ready()
                mixed = save('#s-export', 'two-stem-mix.wav')
                result['checks'].append('two imported stems, gain, mix and raw ZIP')

                # A mismatched replacement import must not erase the previous bank.
                page.locator('.edit-import summary').click()
                page.locator('#s-stem-files input[data-kind="vocals"]').set_input_files(fixture('short.wav', seconds=2))
                page.locator('#s-import').click()
                expect(page.locator('#s-status')).to_contain_text('identical', timeout=30000)
                ready()
                assert save('#s-export', 'after-misaligned-import.wav') == mixed
                assert save('#s-export-stems', 'retained-stems.zip') == raw_two
                result['checks'].append('misaligned import retains existing mix and bank')

                page.locator('#s-mode').select_option('four')
                for kind, hz in [('vocals', 440), ('drums', 110), ('bass', 70), ('other', 880)]:
                    page.locator(f'#s-stem-files input[data-kind="{kind}"]').set_input_files(fixture(f'{kind}.wav', hz))
                page.locator('#s-import').click()
                ready()
                before_bank = zipfile.ZipFile(io.BytesIO(save('#s-export-stems', 'four-stems-before.zip')))
                page.locator('#s-target').select_option('vocals')
                page.locator('#s-tab-voice').click()
                page.locator('#s-voice').click()
                ready()
                expect(page.locator('#s-apply-stem')).to_be_enabled()
                page.locator('#s-undo').click()
                ready()
                expect(page.locator('#s-apply-stem')).to_be_disabled()
                page.locator('#s-redo').click()
                ready()
                expect(page.locator('#s-apply-stem')).to_be_enabled()
                page.locator('#s-apply-stem').click()
                ready()
                after_bank = zipfile.ZipFile(io.BytesIO(save('#s-export-stems', 'four-stems-after.zip')))
                assert before_bank.read('vocals.wav') != after_bank.read('vocals.wav')
                for kind in ['drums', 'bass', 'other']:
                    assert before_bank.read(kind + '.wav') == after_bank.read(kind + '.wav')
                result['checks'].append('four stems and direct vocal edit preserve untouched stems exactly')
                result['checks'].extend(verify_multitrack(page, ready, save, fixture, args.out, diagnostics.capture))
                result['checks'].extend(verify_bundle(page, ready, save, fixture, args.out, diagnostics.capture))

                page.locator('#s-tab-repair').click()
                page.locator('#s-target').select_option('source')
                page.locator('#s-spectrum').click()
                expect(page.locator('#s-status')).to_contain_text('graph shows', timeout=30000)
                page.locator('#s-spectrum-canvas').scroll_into_view_if_needed()
                box = page.locator('#s-spectrum-canvas').bounding_box()
                page.mouse.move(box['x'] + box['width'] * .35, box['y'] + 30)
                page.mouse.down()
                page.mouse.move(box['x'] + box['width'] * .6, box['y'] + 90, steps=5)
                page.mouse.up()
                assert float(page.locator('#s-start').input_value()) < float(page.locator('#s-end').input_value())
                assert float(page.locator('#s-low').input_value()) < float(page.locator('#s-high').input_value())
                for id, value in [('start', '0.8'), ('end', '1.6'), ('low', '300'), ('high', '600')]:
                    page.locator('#s-' + id).fill(value)
                diagnostics.capture(args.out / 'desktop-repair.png', full_page=True)
                page.locator('#s-repair').click()
                ready()
                repaired = save('#s-export', 'spectral-repair.wav')
                page.locator('#s-volume').fill('0.1')
                page.locator('#s-volume').dispatch_event('input')
                assert save('#s-export', 'low-listening-volume.wav') == repaired
                page.locator('#s-play').click()
                expect(page.locator('#s-play')).to_have_text('Pause', timeout=30000)
                page.wait_for_timeout(350)
                page.locator('#s-before').click()
                expect(page.locator('#s-play')).to_have_text('Pause')
                page.locator('#s-stop').click()
                # Real audio contexts can resume asynchronously after a route
                # change. A late resume must never undo the user's Stop.
                page.evaluate("""() => {
                    window.originalResume = AudioContext.prototype.resume;
                    AudioContext.prototype.resume = function() { return new Promise(resolve => {
                        window.releaseStudioResume = () => window.originalResume.call(this).then(resolve);
                    }); };
                }""")
                page.locator('#s-play').click()
                page.wait_for_function('() => typeof window.releaseStudioResume === "function"')
                page.locator('#s-stop').click()
                page.evaluate("""async () => { await window.releaseStudioResume(); AudioContext.prototype.resume = window.originalResume; await new Promise(resolve => setTimeout(resolve, 50)); }""")
                expect(page.locator('#s-play')).to_have_text('Play')
                result['checks'].append('spectral repair, A/B playback and listening-independent export')
                result['checks'].append('late AudioContext resume cannot restart stopped Studio playback')

                page.set_viewport_size({'width': 390, 'height': 844})
                page.locator('#s-spectrum').click()
                expect(page.locator('#s-status')).to_contain_text('graph shows', timeout=30000)
                page.locator('#s-spectrum-canvas').scroll_into_view_if_needed()
                page.locator('#s-spectrum-canvas').evaluate("el => { const r=el.getBoundingClientRect(), dock=document.querySelector('.edit-player').getBoundingClientRect().height; window.scrollTo(0,scrollY+r.top-(innerHeight-dock-r.height)/2); }")
                page.wait_for_timeout(200)
                diagnostics.capture(args.out / 'phone-repair.png')
                diagnostics.capture(args.out / 'phone-full.png', full_page=True)
                assert not page.evaluate('() => document.documentElement.scrollWidth > innerWidth')
                page.locator('#s-export').scroll_into_view_if_needed()
                assert page.locator('#s-export').is_visible()
                result['checks'].append('390px responsive layout and reachable export')
                page.set_viewport_size({'width': 1480, 'height': 1000})
                page.locator('#s-promote').click()
                expect(page.locator('#analyze')).to_be_enabled(timeout=60000)
                expect(page.locator('#studio-tools')).to_be_hidden()
                page.locator('#tab-studio-tools').click()
                ready()
                assert save('#s-export', 'after-master-handoff.wav') == repaired
                page.locator('#s-close').click()
                page.locator('#tab-studio-tools').click()
                expect(page.locator('#s-export')).to_be_disabled()
                result['checks'].append('mastering handoff retains Studio; Close releases workspace')
                assert not any(request['method'] != 'GET' or request['body_bytes'] for request in result['requests'])
                assert not result['errors'], result['errors']
                result['passed'] = True
            except Exception as error:
                result['failure'] = repr(error)
                diagnostics.capture(args.out / 'failure.png', full_page=True)
                result['status'] = page.locator('#s-status').inner_text(timeout=1000) if page.locator('#s-status').count() else 'Studio UI was not initialized'
                raise
            finally:
                (args.out / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
                browser.close()
    finally:
        server.shutdown()
    print(json.dumps({key: value for key, value in result.items() if key != 'requests'}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--site', type=Path, default=ROOT / 'web/dist')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    parser.add_argument('--executable')
    run(parser.parse_args())
