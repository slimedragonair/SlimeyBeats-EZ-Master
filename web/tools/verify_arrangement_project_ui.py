"""Focused real project Save/Open and WAV controls, after editing qualification."""
import argparse
import functools
import hashlib
import http.server
import json
from pathlib import Path
import struct
import threading
import traceback
import zipfile

import numpy as np
from playwright.sync_api import expect, sync_playwright
from browser_capture import BrowserDiagnostics
from verify_arrangement_browser import Handler, fixture


def float_wav(data):
    assert data[:4] == b'RIFF' and data[8:12] == b'WAVE'
    offset, fmt, pcm = 12, None, None
    while offset + 8 <= len(data):
        kind, size = data[offset:offset+4], struct.unpack_from('<I', data, offset+4)[0]
        chunk = data[offset+8:offset+8+size]
        if kind == b'fmt ': fmt = chunk
        if kind == b'data': pcm = chunk
        offset += 8 + size + size % 2
    tag, channels, rate = struct.unpack_from('<HHI', fmt)
    bits = struct.unpack_from('<H', fmt, 14)[0]
    assert bits == 32 and (tag == 3 or tag == 65534 and fmt[24:40] == bytes.fromhex('0300000000001000800000aa00389b71'))
    return np.frombuffer(pcm, dtype='<f4').reshape(-1, channels), rate


def verify(browser, base, width, args, result, source):
    page = browser.new_page(viewport={'width': width, 'height': 844})
    row = {'width': width, 'checks': [], 'errors': [], 'downloads': []}; result['cases'].append(row)
    diagnostics = BrowserDiagnostics(page, args.browser, row)
    page.add_init_script(path=str(Path(__file__).with_name('arrangement_audio_audit.js')))
    page.on('download', lambda download: row['downloads'].append(download.suggested_filename))
    a = lambda name: page.locator(f'[data-arr="{name}"]')
    snapshot = lambda: page.evaluate('()=>arrangementAudit.latest')

    def ready(revision):
        page.wait_for_function('rev=>arrangementAudit.latest?.revision>rev && !arrangementAudit.latest.preview.pending', arg=revision, timeout=30000)
        expect(a('add-files')).to_be_enabled(timeout=30000)
        return snapshot()

    def edit(button):
        previous = snapshot()['revision']; a(button).click(); return ready(previous)

    try:
        page.goto(base, wait_until='networkidle')
        expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
        page.locator('#tab-studio-tools').click()
        expect(page.locator('#s-arrange-entry')).to_be_visible(timeout=30000)
        page.locator('#s-arrange-entry').click()
        a('files').set_input_files(str(source)); edit('import-commit')
        current = snapshot(); lane = current['arrangement']['lanes'][0]
        a('lane-select').select_option(lane['id']); a('clip-select').select_option(lane['clips'][0]['id'])
        a('clip-start').fill('2'); a('clip-source').fill('0.5'); a('clip-end').fill('9.5'); edit('clip-timing')
        a('eq-scope').select_option('range'); a('eq-range-target').select_option('clip')
        a('range-start').fill('2.5'); a('range-end').fill('3.5'); a('range-end').press('Tab')
        a('eq-band').select_option('2'); a('eq-frequency').fill('440'); a('eq-shape').select_option('bell'); a('eq-q').fill('1'); a('eq-gain').fill('-6')
        saved = edit('eq-update')
        assert saved['arrangement']['eq'][0]['range'] == {'start_frame': 48000, 'end_frame': 96000}
        a('file-actions').locator('summary').click()
        with page.expect_download(timeout=30000) as pending:
            a('project-save').click()
        project = args.out / f'{width}-project.zip'; pending.value.save_as(str(project))
        expect(a('project-save')).to_be_enabled(timeout=30000)
        with zipfile.ZipFile(project) as archive:
            manifest = json.loads(archive.read('arrangement.json'))
            assert manifest['version'] == 2 and manifest['history']['current'] == saved['arrangement']
            assert manifest['sources'][0]['id'] == saved['assets'][0]['id']
            assert manifest['history']['undo'] and manifest['undo_labels']
        current = edit('delete-clip'); assert not current['arrangement']['lanes'][0]['clips']
        previous = current['revision']; a('project-file').set_input_files(str(project)); restored = ready(previous)
        assert restored['arrangement'] == saved['arrangement'] and restored['assets'] == saved['assets']
        assert restored['epoch'] != saved['epoch'] and restored['history']['can_undo']
        expect(page.locator('#s-play')).to_have_text('Play')
        a('lane-select').select_option(lane['id']); a('clip-select').select_option(lane['clips'][0]['id'])
        a('eq-scope').select_option('range'); a('eq-range-target').select_option('clip')
        a('range-start').fill('2.5'); a('range-end').fill('3.5'); a('range-end').press('Tab')
        a('eq-band').select_option('2'); expect(a('eq-gain')).to_have_value('-6')
        previous = restored['revision']; a('undo').click(); undone = ready(previous)
        assert not undone['arrangement']['eq']
        previous = undone['revision']; a('redo').click(); restored = ready(previous)
        assert restored['arrangement'] == saved['arrangement']
        row['checks'].append('explicit project ZIP restores source/effect IDs, source-frame range EQ and usable Undo/Redo without autoplay')

        damaged = args.out / f'{width}-corrupt.zip'
        contents = bytearray(project.read_bytes()); contents[70] ^= 1; damaged.write_bytes(contents)
        previous = restored['revision']; a('project-file').set_input_files(str(damaged))
        expect(a('status')).to_have_class('arr-status error', timeout=30000)
        assert snapshot()['revision'] == previous and snapshot()['arrangement'] == saved['arrangement']
        row['checks'].append('corrupt project reports an error while preserving the existing playable document')

        page.locator('#s-playback-details > summary').click()
        page.locator('#s-volume').fill('0.35'); page.locator('#s-seek').fill('2.7')
        page.locator('#s-play').click(); expect(page.locator('#s-play')).to_have_text('Pause')
        live = None
        if getattr(args, 'capture_live', True):
            page.evaluate('()=>arrangementAudit.attach()'); page.evaluate('()=>arrangementAudit.reset()')
            page.wait_for_function('()=>arrangementAudit.frames>=18000', timeout=10000)
            live = page.evaluate('()=>arrangementAudit.measure()')
        else:
            # Hosted sites have no private observer module. Keep that pass to
            # real transport progress and saved PCM, with no live-PCM claim.
            page.wait_for_function("()=>Number(document.querySelector('#s-seek').value)>2.95", timeout=10000)
        page.locator('#s-stop').click()
        a('file-actions').locator('summary').click() if not a('file-actions').evaluate('el=>el.open') else None
        a('audio-format').select_option('wav32f')
        with page.expect_download(timeout=30000) as pending:
            a('audio-export').click()
        exported = args.out / f'{width}-arrangement.wav'; pending.value.save_as(str(exported))
        expect(a('audio-export')).to_be_enabled(timeout=30000)
        pcm, rate = float_wav(exported.read_bytes())
        assert pcm.shape == (456000, 2) and rate == 48000 and np.isfinite(pcm).all()
        assert np.max(np.abs(pcm[:96000])) < 1e-8
        amplitude = lambda first: 2 * abs(np.sum(pcm[first:first+9600, 0] * np.exp(-2j*np.pi*440*np.arange(9600)/rate))) / 9600
        clear, reduced = amplitude(100800), amplitude(134400)
        assert abs(reduced / clear - 10 ** (-6/20)) < .005
        if live is not None:
            assert abs(live['first'] / reduced - .35) < .025, (live, reduced)
        assert row['downloads'] == ['Studio-arrangement.zip', 'Studio-arrangement.wav']
        assert not row['errors'], row
        row['measurements'] = {'export_unfiltered_amplitude': clear, 'export_scoped_amplitude': reduced,
                               'live_scoped_amplitude': live['first'] if live else None}
        row['live_pcm_qualified'] = live is not None
        row['checks'].append('explicit float WAV has exact duration and scoped EQ; actual live output matches with listening volume excluded from saved PCM'
                             if live else 'real playback clock advances; explicit Float32 WAV independently verifies exact duration, silence and scoped EQ; live PCM is not captured in this hosted pass')
        a('eq-plot').scroll_into_view_if_needed(); diagnostics.capture(args.out / f'{width}-restored.png')
        row['passed'] = True
    except Exception as error:
        row.update(passed=False, failure=str(error), traceback=traceback.format_exc(), status=page.locator('#s-status').inner_text())
        diagnostics.capture(args.out / f'{width}-failure.png'); raise
    finally:
        page.close()


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--site', type=Path, required=True); parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium'); args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True); source = args.out / 'immutable.wav'; digest = fixture(source, 440, 8, 1)
    result = {'scope': 'Actual UI/project and shared-WASM WAV parity; synthetic device, no physical-device qualification', 'fixture_sha256': digest, 'cases': []}
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    try:
        with sync_playwright() as p:
            browser = getattr(p, args.browser).launch(headless=True); result['browser'] = f'{args.browser} {browser.version}'
            try:
                for width in [320, 390]: verify(browser, f'http://127.0.0.1:{server.server_address[1]}/', width, args, result, source)
            finally: browser.close()
        assert hashlib.sha256(source.read_bytes()).hexdigest() == digest
        result['passed'] = True
    finally:
        server.shutdown(); server.server_close(); thread.join()
        (args.out / 'results.json').write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8'); print(json.dumps(result, indent=2))


if __name__ == '__main__': main()
