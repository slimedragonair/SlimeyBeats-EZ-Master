"""Actual MP3 download through the existing encoder; project ZIP stays separate."""
import argparse
import functools
import hashlib
import http.server
import json
from pathlib import Path
import shutil
import subprocess
import threading
import traceback
import zipfile

import numpy as np
from playwright.sync_api import expect, sync_playwright
from browser_capture import BrowserDiagnostics
from verify_arrangement_browser import Handler, fixture


def verify(browser, base, width, args, result, sources):
    page = browser.new_page(viewport={'width': width, 'height': 844})
    row = {'width': width, 'checks': [], 'errors': [], 'downloads': []}; result['cases'].append(row)
    diag = BrowserDiagnostics(page, args.browser, row)
    page.add_init_script(path=str(Path(__file__).with_name('arrangement_audio_audit.js')))
    page.on('download', lambda download: row['downloads'].append(download.suggested_filename))
    a = lambda name: page.locator(f'[data-arr="{name}"]')
    try:
        page.goto(base, wait_until='networkidle')
        expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
        page.locator('#tab-studio-tools').click(); expect(page.locator('#s-arrange-entry')).to_be_visible(timeout=30000)
        page.locator('#s-arrange-entry').click(); a('files').set_input_files([str(source) for source in sources]); a('import-commit').click()
        expect(page.locator('#s-play')).to_be_enabled(timeout=30000)
        saved = page.evaluate('()=>arrangementAudit.latest'); lanes = saved['arrangement']['lanes']
        proportions = page.evaluate('ids=>ids.map(id=>Number.parseFloat(document.querySelector(`[data-clip-id="${id}"]`).style.width))', [lane['clips'][0]['id'] for lane in lanes])
        assert proportions == [100, 75], proportions
        a('lanes').scroll_into_view_if_needed(); diag.capture(args.out / f'{width}-first-import.png')
        row['checks'].append('first import fits the entire eight-second song and visibly preserves the six-second part length')
        a('lane-select').select_option(lanes[0]['id']); a('clip-select').select_option(lanes[0]['clips'][0]['id'])
        a('lane-inspector').locator('summary').click(); a('lane-gain').fill('-6'); a('lane-update').click()
        page.wait_for_function('rev=>arrangementAudit.latest.revision>rev && !arrangementAudit.latest.preview.pending', arg=saved['revision'], timeout=30000)
        expect(a('add-files')).to_be_enabled(timeout=30000)
        saved = page.evaluate('()=>arrangementAudit.latest'); document = saved['arrangement']
        a('file-actions').locator('summary').click(); a('audio-format').select_option('mp3')
        expect(a('mp3-note')).to_be_visible(); expect(a('mp3-note')).to_contain_text('No mastering pass')
        with page.expect_download(timeout=30000) as pending: a('project-save').click()
        project = args.out / f'{width}-project.zip'; pending.value.save_as(str(project)); expect(a('project-save')).to_be_enabled(timeout=30000)
        with zipfile.ZipFile(project) as archive:
            manifest = json.loads(archive.read('arrangement.json'))
            assert manifest['history']['current'] == document and manifest['version'] == 2
            assert all(name.endswith('.wav') for name in archive.namelist() if name.startswith('assets/'))
        expect(a('audio-format')).to_have_value('mp3')
        row['checks'].append('Save project remains canonical source/history ZIP while MP3 is selected for delivery')
        with page.expect_download(timeout=120000) as pending: a('audio-export').click()
        mp3 = args.out / f'{width}-arrangement.mp3'; pending.value.save_as(str(mp3)); expect(a('audio-export')).to_be_enabled(timeout=30000)
        assert page.evaluate('()=>arrangementAudit.latest.arrangement') == document
        measured = page.evaluate('()=>arrangementAudit.measuredOutputs.at(-1)')
        assert measured and measured['sample_rate'] == 48000 and measured['channels'] == 2
        assert measured['metrics']['true_peak_dbtp'] <= -1 + .005, measured
        deliveries = page.evaluate('()=>arrangementAudit.deliveries')
        assert deliveries[-2:] == [{'name': 'Studio-arrangement.zip', 'type': 'application/zip'}, {'name': 'Studio-arrangement.mp3', 'type': 'audio/mpeg'}], deliveries
        ffprobe, ffmpeg = shutil.which('ffprobe'), shutil.which('ffmpeg')
        if not ffprobe or not ffmpeg: raise RuntimeError('Independent MP3 qualification requires the existing FFmpeg tools')
        metadata = json.loads(subprocess.run([ffprobe, '-v', 'error', '-select_streams', 'a:0', '-show_entries', 'stream=sample_rate,channels,bit_rate,duration', '-of', 'json', str(mp3)], capture_output=True, check=True).stdout)['streams'][0]
        assert int(metadata['sample_rate']) == 48000 and metadata['channels'] == 2 and int(metadata['bit_rate']) == 320000, metadata
        raw = subprocess.run([ffmpeg, '-v', 'error', '-i', str(mp3), '-f', 'f32le', '-acodec', 'pcm_f32le', '-'], capture_output=True, check=True).stdout
        pcm = np.frombuffer(raw, dtype='<f4').reshape(-1, 2)
        assert 384000 <= len(pcm) <= 396000 and np.isfinite(pcm).all()
        tone = lambda hz: 2 * abs(np.sum(pcm[48000:57600, 0] * np.exp(-2j*np.pi*hz*np.arange(9600)/48000))) / 9600
        first, second = tone(440), tone(880)
        assert abs(first - .06 * 10 ** (-6/20)) < .001 and abs(second - .06) < .001, (first, second)
        actions = page.evaluate('()=>arrangementAudit.actions')
        assert not any(action in ['master', 'master_preview', 'load'] for action in actions)
        layout = page.evaluate('()=>({client:document.documentElement.clientWidth,scroll:document.documentElement.scrollWidth})')
        assert layout['scroll'] <= layout['client'] + 1
        assert not row['errors'] and row['downloads'] == ['Studio-arrangement.zip', 'Studio-arrangement.mp3']
        a('audio-format').scroll_into_view_if_needed(); diag.capture(args.out / f'{width}-mp3-format.png')
        row.update(passed=True, measurements={'first_tone': first, 'second_tone': second, 'decoded_frames': len(pcm), 'metadata': metadata}, encoded_metrics=measured['metrics'], layout=layout)
        row['checks'].append('actual 320 kbps audio/mpeg download verifies after encoding, retains applied gain/other track and does not remaster or mutate the document')
    except Exception as error:
        row.update(passed=False, failure=str(error), traceback=traceback.format_exc(), status=page.locator('#s-status').inner_text())
        diag.capture(args.out / f'{width}-failure.png'); raise
    finally: page.close()


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--site', type=Path, required=True); parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium'); args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True); sources = [args.out / 'first.wav', args.out / 'second.wav']
    hashes = [fixture(sources[0], 440, 8, 1), fixture(sources[1], 880, 6, 2)]
    result = {'scope': 'Actual responsive UI download, shared encoded metrics and independent FFmpeg decode; synthetic sources', 'cases': [], 'fixture_sha256': hashes}
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site))); thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    try:
        with sync_playwright() as p:
            browser = getattr(p, args.browser).launch(headless=True); result['browser'] = f'{args.browser} {browser.version}'
            try:
                for width in [320, 390]: verify(browser, f'http://127.0.0.1:{server.server_address[1]}/', width, args, result, sources)
            finally: browser.close()
        assert [hashlib.sha256(path.read_bytes()).hexdigest() for path in sources] == hashes
        result['passed'] = True
    finally:
        server.shutdown(); server.server_close(); thread.join(); (args.out / 'results.json').write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8'); print(json.dumps(result, indent=2))


if __name__ == '__main__': main()
