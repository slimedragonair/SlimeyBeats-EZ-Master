"""Actual responsive Studio controls, shared WASM and worklet output.

Uses synthetic sources, private contexts and no download/export requests. A
headless synthetic audio device does not qualify physical mobile playback.
"""
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
import wave
import zipfile

import numpy as np
from playwright.sync_api import expect, sync_playwright
from browser_capture import BrowserDiagnostics

ROOT = Path(__file__).resolve().parents[2]


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_unused):
        pass

    def translate_path(self, path):
        if path == '/__arrangement_capture.js':
            return str(Path(__file__).with_name('studio_capture_processor.js'))
        return super().translate_path(path)


def fixture(path, hz, seconds, channels, bright=False):
    rate, frames = 48000, 48000 * seconds
    samples = .06 * np.sin(np.arange(frames) * (2 * np.pi * hz / rate))
    if bright:
        spectrum = np.fft.rfft(np.random.default_rng(731).standard_normal(frames))
        frequencies = np.fft.rfftfreq(frames, 1 / rate)
        for low, high, level in [(10000, 16000, .05), (2000, 8000, .012)]:
            noise = np.fft.irfft(spectrum * ((frequencies >= low) & (frequencies <= high)), n=frames)
            samples += level * noise / np.sqrt(np.mean(noise * noise))
    assert np.max(np.abs(samples)) < .9
    pcm = samples if channels == 1 else np.column_stack([samples, samples * .8])
    with wave.open(str(path), 'wb') as writer:
        writer.setparams((channels, 2, rate, 0, 'NONE', 'not compressed'))
        writer.writeframes(np.round(pcm * 32767).astype('<i2').tobytes())
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(browser, base, width, args, result, sources, archive):
    context = browser.new_context(viewport={'width': width, 'height': 844})
    page = context.new_page()
    row = {'width': width, 'checks': [], 'errors': [], 'downloads': [], 'measurements': {}}
    result['cases'].append(row)
    diagnostics = BrowserDiagnostics(page, args.browser, row)
    page.add_init_script(path=str(Path(__file__).with_name('arrangement_audio_audit.js')))
    page.on('download', lambda download: row['downloads'].append(download.suggested_filename))
    a = lambda name: page.locator(f'[data-arr="{name}"]')

    def snapshot():
        return page.evaluate('()=>arrangementAudit.latest')

    def updated(revision):
        page.wait_for_function('rev=>arrangementAudit.latest?.revision>rev && !arrangementAudit.latest.preview.pending', arg=revision, timeout=30000)
        expect(a('add-files')).to_be_enabled(timeout=30000)
        return snapshot()

    def edit(button):
        revision = snapshot()['revision']
        a(button).click()
        return updated(revision)

    def measure(name):
        page.evaluate('()=>arrangementAudit.reset()')
        page.wait_for_function('()=>arrangementAudit.frames>=18000', timeout=10000)
        value = page.evaluate('()=>arrangementAudit.measure()')
        row['measurements'][name] = value
        return value

    try:
        page.goto(base, wait_until='networkidle')
        expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
        page.locator('#tab-studio-tools').click()
        expect(page.locator('#s-arrange-entry')).to_be_visible(timeout=30000)
        page.locator('#s-arrange-entry').click()
        expect(a('workspace')).to_be_visible()
        expect(page.locator('#s-legacy-workspace')).to_be_hidden()
        expect(page.locator('#s-play')).to_be_disabled()
        a('files').set_input_files([str(source) for source in sources])
        expect(a('import-parts').locator('input[type=checkbox]')).to_have_count(2)
        current = edit('import-commit')
        lanes = current['arrangement']['lanes']
        assert len(lanes) == 2 and {lane['clips'][0]['end_frame'] for lane in lanes} == {384000, 288000}, lanes
        assert {asset['channels'] for asset in current['assets']} == {1, 2}, current['assets']
        first, second = lanes
        page.locator('#s-before').click()
        expect(page.locator('#s-play')).to_be_disabled()
        expect(page.locator('#s-after')).to_be_enabled()
        page.locator('#s-after').click()
        expect(page.locator('#s-play')).to_be_enabled()
        a('lane-select').select_option(first['id'])
        a('clip-select').select_option(first['clips'][0]['id'])
        page.wait_for_function('()=>arrangementAudit.actions.includes("waveform")')
        layout = page.evaluate('''()=>({client:document.documentElement.clientWidth,scroll:document.documentElement.scrollWidth,
            viewport:visualViewport.width,player:document.querySelector('.edit-player').getBoundingClientRect().height})''')
        assert layout['scroll'] <= layout['client'] + 1 <= layout['viewport'] + 1, layout
        row['checks'].append('actual WAV imports retain differing durations and mono/stereo assets in separate responsive lanes')

        a('clip-start').fill('1.125'); a('clip-source').fill('0.5'); a('clip-end').fill('8.125')
        current = edit('clip-timing')
        clip = current['arrangement']['lanes'][0]['clips'][0]
        assert [clip['start_frame'], clip['source_start_frame'], clip['end_frame']] == [54000, 24000, 390000], clip
        revision = current['revision']; a('undo').click(); current = updated(revision)
        assert current['arrangement']['lanes'][0]['clips'][0]['start_frame'] == 0
        revision = current['revision']; a('redo').click(); current = updated(revision)
        assert current['arrangement']['lanes'][0]['clips'][0]['start_frame'] == 54000
        revision = current['revision']; a('undo').click(); updated(revision)
        row['checks'].append('exact timing and Undo/Redo execute real typed core transactions without replacing immutable assets')

        page.locator('#s-play').click()
        expect(page.locator('#s-play')).to_have_text('Pause')
        page.wait_for_function('()=>arrangementAudit.outputs.length>0')
        page.evaluate('()=>arrangementAudit.attach()')
        baseline = measure('initial actual PCM')
        assert baseline['first'] > .025 and baseline['second'] > .025, baseline
        a('lane-inspector').locator('summary').click()
        a('lane-name').fill('')
        a('lane-name').press_sequentially('Shared drums', delay=40)
        expect(a('lane-name')).to_have_value('Shared drums')
        current = edit('lane-update')
        assert current['arrangement']['lanes'][0]['name'] == 'Shared drums'
        a('lane-gain').fill('-6'); edit('lane-update')
        expect(page.locator('#s-play')).to_have_text('Pause')
        lower = measure('live lane gain')
        assert abs(lower['first'] / baseline['first'] - 10 ** (-6 / 20)) < .025, lower
        assert abs(lower['second'] / baseline['second'] - 1) < .025, lower
        page.locator('#s-before').click()
        expect(page.locator('#s-play')).to_have_text('Pause')
        before = measure('running Before last edit')
        assert abs(before['first'] / baseline['first'] - 1) < .025, before
        page.locator('#s-after').click()
        expect(page.locator('#s-play')).to_have_text('Pause')
        after = measure('running current arrangement')
        assert abs(after['first'] / lower['first'] - 1) < .025, after
        page.locator('#s-stop').click()
        expect(page.locator('#s-play')).to_have_text('Play')
        silent = measure('Stop actual silence')
        assert silent['peak'] < 1e-7, silent
        page.locator('#s-before').click()
        expect(page.locator('#s-play')).to_have_text('Play')
        page.locator('#s-after').click()
        expect(page.locator('#s-play')).to_have_text('Play')
        row['checks'].append('live gain changes actual output; running A/B remains running, paused A/B stays paused, Stop emits silence')

        a('eq-scope').select_option('lane')
        a('eq-band').select_option('2')
        a('eq-frequency').fill('440'); a('eq-shape').select_option('bell'); a('eq-q').fill('1'); a('eq-gain').fill('-12')
        current = edit('eq-update')
        effect = current['arrangement']['eq'][0]
        assert effect['scope'] == {'kind': 'lane', 'lane_id': first['id']}, effect
        page.locator('#s-play').click(); eq_pcm = measure('lane scoped EQ actual PCM'); page.locator('#s-stop').click()
        assert eq_pcm['first'] < lower['first'] * .4 and abs(eq_pcm['second'] / baseline['second'] - 1) < .035, eq_pcm
        a('eq-gain').fill('-6'); current = edit('eq-update')
        assert len(current['arrangement']['eq']) == 1 and current['arrangement']['eq'][0]['id'] == effect['id']
        a('eq-scope').select_option('clip'); a('eq-gain').fill('-2'); edit('eq-update')
        a('eq-scope').select_option('lane')
        expect(a('eq-gain')).to_have_value('-6')
        a('clip-start').fill('1'); a('clip-source').fill('0.5'); a('clip-end').fill('8.5'); edit('clip-timing')
        a('eq-scope').select_option('range'); a('eq-range-target').select_option('clip')
        a('range-start').fill('1.5'); a('range-end').fill('2.5'); a('range-end').press('Tab')
        a('eq-gain').fill('-3'); current = edit('eq-update')
        ranged = current['arrangement']['eq'][-1]
        assert ranged['scope']['kind'] == 'clip' and ranged['range'] == {'start_frame': 48000, 'end_frame': 96000}, ranged
        row['checks'].append('scoped EQ changes real PCM; repeated updates retain effect IDs; returning scopes reload saved values and clip ranges use source frames')

        a('eq-scope').select_option('lane')
        revision = current['revision']
        a('eq-suggest').click()
        expect(a('eq-apply-proposal')).to_be_visible(timeout=30000)
        assert snapshot()['revision'] == revision
        assert not a('eq-details').evaluate('el=>el.open')
        assert len(a('eq-note').inner_text()) < 180
        expect(a('eq-plot')).to_be_visible()
        a('eq-plot').scroll_into_view_if_needed(); diagnostics.capture(args.out / f'{width}-proposal.png')
        current = edit('eq-apply-proposal')
        assert len(current['arrangement']['eq']) == 3 and current['arrangement']['eq'][0]['id'] == effect['id']
        proposal = page.evaluate('()=>arrangementAudit.proposals.at(-1)')
        assert len(proposal['bands']) == 7 and proposal['high_frequency']['excess_candidate'], proposal
        row['high_frequency'] = proposal['high_frequency']
        row['checks'].append('actual scoped proposal shows current/proposed response and compact details before explicit Apply updates the existing seven-band effect')

        a('lane-select').select_option(first['id']); a('clip-select').select_option(first['clips'][0]['id'])
        if not page.locator('#s-playback-details').evaluate('el=>el.open'): page.locator('#s-playback-details > summary').click()
        page.locator('#s-seek').fill('2')
        expect(page.locator('#s-seek')).to_have_value('2')
        current = edit('split'); assert len(current['arrangement']['lanes'][0]['clips']) == 2
        current = edit('duplicate'); assert len(current['arrangement']['lanes'][0]['clips']) == 3
        current = edit('delete-clip'); assert len(current['arrangement']['lanes'][0]['clips']) == 2
        revision = current['revision']; a('undo').click(); current = updated(revision)
        assert len(current['arrangement']['lanes'][0]['clips']) == 3
        current = edit('lane-down'); assert current['arrangement']['lanes'][1]['id'] == first['id']
        a('lane-select').select_option(second['id'])
        current = edit('delete-lane'); assert len(current['arrangement']['lanes']) == 1
        revision = current['revision']; a('undo').click(); current = updated(revision)
        assert len(current['arrangement']['lanes']) == 2
        if archive:
            revision = current['revision']
            a('zip').set_input_files(str(archive))
            expect(a('import-parts').locator('input[type=checkbox]')).to_have_count(2)
            versions = a('import-parts').locator('select')
            expect(versions.first.locator('option')).to_have_count(2)
            assert versions.first.locator('option:checked').inner_text().lower().endswith('.wav')
            a('import-parts').locator('input[type=checkbox]').nth(1).uncheck()
            expect(a('import-omissions')).to_contain_text('Left out:')
            assert snapshot()['revision'] == revision
            current = edit('import-commit'); assert len(current['arrangement']['lanes']) == 3
            row['checks'].append('actual ZIP review prefers WAV, preserves one encoding per part and applies only the explicitly selected part')
        page.locator('#s-legacy-entry').click(); expect(page.locator('#s-legacy-workspace')).to_be_visible()
        page.locator('#s-arrange-entry').click(); expect(a('workspace')).to_be_visible()
        expect(page.locator('#s-play')).to_have_text('Play')
        assert not any(action.startswith('export') or action == 'asset-wav' for action in page.evaluate('()=>arrangementAudit.actions'))
        assert not row['errors'] and not row['downloads'], row
        row['checks'].append('typing survives live synchronization; split, duplicate, delete, track reorder/removal and Undo reach the core; pane switches preserve work and never export audio')
        row.update(passed=True, layout=layout, final_revision=current['revision'])
    except Exception as error:
        row.update(passed=False, failure=str(error), traceback=traceback.format_exc())
        row['studio_status'] = page.locator('#s-status').inner_text()
        row['last_adapter_state'] = page.evaluate('''()=>({revision:arrangementAudit.latest?.revision,preview:arrangementAudit.latest?.preview,
            history:arrangementAudit.latest?.history,preparations:arrangementAudit.preparations.slice(-6),actions:arrangementAudit.actions.slice(-20)})''')
        diagnostics.capture(args.out / f'{width}-failure.png')
        raise
    finally:
        context.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--site', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    args = parser.parse_args(); args.out.mkdir(parents=True, exist_ok=True)
    sources = [args.out / 'bright-part.wav', args.out / 'second-part.wav']
    hashes = {str(sources[0]): fixture(sources[0], 440, 8, 1, True), str(sources[1]): fixture(sources[1], 880, 6, 2)}
    mp3, archive = args.out / 'layer.mp3', args.out / 'parts.zip'
    ffmpeg = shutil.which('ffmpeg')
    if not ffmpeg: raise RuntimeError('FFmpeg is required to build the synthetic ZIP encoding-review fixture')
    subprocess.run([ffmpeg, '-hide_banner', '-loglevel', 'error', '-y', '-i', str(sources[0]), '-c:a', 'libmp3lame', '-b:a', '192k', str(mp3)], check=True)
    with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as writer:
        writer.write(sources[0], 'wav/layer.wav'); writer.write(mp3, 'mp3/layer.mp3'); writer.write(sources[1], 'wav/extra.wav')
        writer.writestr('notes.txt', 'Synthetic arrangement qualification fixture')
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    result = {'scope': 'Actual shared WASM and headless worklet output; synthetic device, no physical mobile/listening qualification', 'cases': [], 'fixture_sha256': hashes}
    try:
        with sync_playwright() as p:
            browser = getattr(p, args.browser).launch(headless=True)
            result['browser'] = f'{args.browser} {browser.version}'
            try:
                for width in [320, 390]: verify(browser, f'http://127.0.0.1:{server.server_address[1]}/', width, args, result, sources, archive)
            finally:
                browser.close()
        assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest for path, digest in hashes.items())
        result['passed'] = True
    finally:
        server.shutdown(); server.server_close(); thread.join()
        (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
        print(json.dumps(result, indent=2))


if __name__ == '__main__': main()
