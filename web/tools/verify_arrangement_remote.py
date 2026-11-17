"""Actual Server On arrangement controls, authenticated MP3 stream and PCM."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys
import time
import traceback

from playwright.sync_api import expect, sync_playwright
from arrangement_remote_diagnostics import ArrangementRemoteDiagnostics
from remote_browser_fixture import RemoteBrowserFixture
from verify_arrangement_browser import fixture
from arrangement_remote_files import verify_files, verify_guest_limit
from arrangement_remote_archive import make_archive, verify_archive
from native_media_capture import native_browser_audio_environment, native_media_capture


def verify(browser, origin, width, args, result, sources, requests):
    request_mark = len(requests)
    context = browser.new_context(viewport={'width': width, 'height': 844})
    page = context.new_page(); row = {'width': width, 'checks': [], 'errors': [], 'downloads': [], 'measurements': {}}
    result['cases'].append(row); diag = ArrangementRemoteDiagnostics(page, args.browser, row)
    page.add_init_script(path=str(Path(__file__).with_name('arrangement_audio_audit.js')))
    page.add_init_script(path=str(Path(__file__).with_name('arrangement_remote_audit.js')))
    page.on('download', lambda item: row['downloads'].append(item.suggested_filename))
    local = lambda name: page.locator(f'#s-arrangement-page [data-arr="{name}"]')
    remote = lambda name: page.locator(f'#s-arrangement-server-page [data-arr="{name}"]')
    snapshot = lambda: page.evaluate('()=>arrangementRemoteAudit.latest')
    def changed(revision):
        page.wait_for_function('rev=>arrangementRemoteAudit.latest?.revision>rev && !arrangementRemoteAudit.latest.preview.pending', arg=revision, timeout=45000)
        expect(remote('add-files')).to_be_enabled(timeout=45000); return snapshot()
    def edit(action):
        revision = snapshot()['revision']; remote(action).click(); return changed(revision)
    def measure(name, audible=True):
        if audible: page.wait_for_function('()=>arrangementRemoteAudit.mediaState().some(value=>!value.paused&&value.time>.25)', timeout=15000)
        value = args.monitor.measure_tones()
        value['media'] = page.evaluate('()=>arrangementRemoteAudit.mediaState()')
        if audible: assert value['peak'] > 1e-5, value
        row['measurements'][name] = value; return value
    def settled(name, predicate):
        started = time.monotonic(); samples = []
        while time.monotonic() - started < 4:
            value = measure(name); samples.append(value)
            if predicate(value):
                value['settle_seconds'] = time.monotonic() - started
                value['settle_samples'] = len(samples)
                return value
        raise AssertionError({'buffered_preview_did_not_settle_within_seconds': 4, 'samples': samples})
    try:
        page.goto(origin, wait_until='networkidle'); expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=45000)
        page.locator('#tab-studio-tools').click(); expect(page.locator('#s-arrange-entry')).to_be_visible(timeout=30000)
        page.locator('#s-arrange-entry').click()
        if args.unavailable:
            local('files').set_input_files(str(sources[0])); local('import-commit').click()
            expect(page.locator('#s-play')).to_be_enabled(timeout=30000)
        local_doc = page.evaluate('()=>arrangementAudit.latest.arrangement')
        worker_mark = page.evaluate('()=>arrangementAudit.actions.length')
        page.locator('#hybrid-enabled').check(); expect(page.locator('#hybrid-availability')).to_have_text('[Server online]', timeout=30000)
        expect(local('add-files')).to_be_disabled(); expect(page.locator('#s-play')).to_be_disabled()
        expect(page.locator('#s-arrange-entry')).to_contain_text('Server'); page.locator('#s-arrange-entry').click()
        if args.unavailable:
            expect(page.locator('#s-status')).to_contain_text('has not enabled arrangement playback', timeout=30000)
            expect(local('workspace')).to_be_visible(); expect(local('add-files')).to_be_disabled()
            assert page.evaluate('()=>arrangementAudit.latest.arrangement') == local_doc
            assert not any(item['method'] == 'POST' and item['path'] == '/v2/projects' for item in requests)
            diag.capture(args.out / f'{width}-unavailable.png')
            page.locator('#hybrid-enabled').uncheck(); expect(local('add-files')).to_be_enabled(); expect(page.locator('#s-play')).to_be_enabled()
            assert page.evaluate('()=>arrangementAudit.latest.arrangement') == local_doc
            row.update(passed=True); row['checks'].append('default-disabled server creates no remote project, preserves local work and locks local processing until Server Off')
            return
        page.wait_for_function('()=>document.querySelector("#s-arrangement-server-page [data-arr=workspace]")?.getBoundingClientRect().height>0 || document.querySelector("#s-status.error")', timeout=45000)
        expect(remote('workspace')).to_be_visible(timeout=1000); expect(remote('layout')).to_contain_text('Server arrangement')
        page.evaluate('()=>arrangementRemoteAudit.serverPhase=true')
        assert page.evaluate('()=>arrangementAudit.latest.arrangement') == local_doc
        expect(page.locator('#s-play')).to_be_disabled()
        remote('files').set_input_files([str(source) for source in sources]); expect(remote('import-parts').locator('input[type=checkbox]')).to_have_count(2)
        current = edit('import-commit'); lanes = current['arrangement']['lanes']
        assert len(lanes) == 2 and [lane['clips'][0]['end_frame'] for lane in lanes] == [960000, 576000], lanes
        assert not any('/arrangement/streams/' in item['path'] and item['method'] == 'GET' for item in requests[request_mark:]), requests[-10:]
        row['checks'].append('Server On uses the same controls, retains local work and admits owned files atomically without starting audio')
        page.locator('#s-before').click(); expect(page.locator('#s-play')).to_be_disabled(); expect(page.locator('#s-after')).to_be_enabled()
        page.locator('#s-after').click(); expect(page.locator('#s-play')).to_be_enabled()
        page.locator('#s-play').click(); expect(page.locator('#s-play')).to_have_text('Pause', timeout=45000)
        baseline = measure('initial server PCM'); assert baseline['first'] > .025 and baseline['second'] > .025, baseline
        remote('lane-select').select_option(lanes[0]['id']); remote('clip-select').select_option(lanes[0]['clips'][0]['id'])
        remote('lane-inspector').locator('summary').click(); remote('lane-gain').fill('-6'); current = edit('lane-update')
        expect(page.locator('#s-play')).to_have_text('Pause')
        lower = settled('running server gain', lambda value: abs(value['first'] / baseline['first'] - 10 ** (-6/20)) < .04 and abs(value['second'] / baseline['second'] - 1) < .04)
        page.locator('#s-before').click(); expect(page.locator('#s-play')).to_have_text('Pause', timeout=45000)
        before = measure('running server Before last edit'); assert abs(before['first'] / baseline['first'] - 1) < .04, before
        page.locator('#s-after').click(); expect(page.locator('#s-play')).to_have_text('Pause', timeout=45000)
        after = measure('running server B'); assert abs(after['first'] / lower['first'] - 1) < .04, after
        page.locator('#s-stop').click(); expect(page.locator('#s-play')).to_have_text('Play'); silent = measure('server Stop', False); assert silent['peak'] < 1e-7
        row['checks'].append('actual compressed output reflects live gain and exclusive running A/B; Stop produces silence')
        remote('eq-scope').select_option('lane'); remote('eq-band').select_option('2'); remote('eq-frequency').fill('440')
        remote('eq-shape').select_option('bell'); remote('eq-q').fill('1'); remote('eq-gain').fill('-12'); current = edit('eq-update')
        assert current['arrangement']['eq'][0]['scope'] == {'kind': 'lane', 'lane_id': lanes[0]['id']}
        page.locator('#s-play').click(); expect(page.locator('#s-play')).to_have_text('Pause', timeout=45000)
        eq = settled('server scoped EQ', lambda value: value['first'] < lower['first'] * .4 and abs(value['second'] / baseline['second'] - 1) < .04)
        page.locator('#s-stop').click()
        if not page.locator('#s-playback-details').evaluate('el=>el.open'): page.locator('#s-playback-details > summary').click()
        page.locator('#s-seek').fill('2.12'); expect(page.locator('#s-seek')).to_have_value('2.12')
        page.wait_for_function('()=>arrangementRemoteAudit.streams.at(-1)?.start_frame===101760', timeout=15000)
        expect(page.locator('#s-play')).to_have_text('Play')
        remote('clip-start').fill('1.125'); remote('clip-source').fill('0.5'); remote('clip-end').fill('20.625'); current = edit('clip-timing')
        assert current['arrangement']['lanes'][0]['clips'][0]['source_start_frame'] == 24000
        previous = current['revision']; remote('undo').click(); current = changed(previous)
        assert current['arrangement']['lanes'][0]['clips'][0]['start_frame'] == 0
        row['checks'].append('scoped EQ changes real PCM, paused frame seek stays paused, exact timing and Undo use the native recipe')
        remote('lanes').scroll_into_view_if_needed(); diag.capture(args.out / f'{width}-server.png')
        page.locator('#hybrid-enabled').uncheck(); expect(local('workspace')).to_be_visible(timeout=15000)
        assert page.evaluate('()=>arrangementAudit.latest.arrangement') == local_doc; expect(page.locator('#s-play')).to_have_text('Play')
        page.locator('#hybrid-enabled').check(); expect(page.locator('#hybrid-availability')).to_have_text('[Server online]', timeout=30000)
        page.locator('#s-arrange-entry').click(); expect(remote('workspace')).to_be_visible(timeout=30000)
        assert snapshot()['arrangement'] == current['arrangement']
        summary = page.evaluate('()=>arrangementRemoteAudit.summary()')
        assert summary['credentials_memory_only'] and summary['fullMediaReads'] == 0, summary
        assert summary['clientDecodes'] == 0 and summary['serverMediaSourceConnections'] == 0, summary
        actions = page.evaluate('start=>arrangementAudit.actions.slice(start)', worker_mark)
        assert not any(action in ['stage-wav', 'stage-asset', 'render-block', 'export-wav', 'master', 'master_preview'] for action in actions), actions
        assert not row['downloads'] and not row['errors'], row
        layout = page.evaluate('()=>({client:document.documentElement.clientWidth,scroll:document.documentElement.scrollWidth})'); assert layout['scroll'] <= layout['client'] + 1
        row['checks'].append('Server Off restores retained local work; Server On restores server edits without PCM copies, token URLs or exports')
        if args.files:
            current = verify_files(page, remote, row, args.out, width, args.ffmpeg, snapshot, changed, edit)
            assert row['downloads'] == ['Studio-arrangement.zip', 'Studio-arrangement.wav', 'Studio-arrangement.mp3'], row['downloads']
            assert page.evaluate('()=>arrangementRemoteAudit.summary().fullMediaReads') == 0
            assert not row['errors'], row
        if args.archive:
            current = verify_archive(page, remote, row, args.archive_path, snapshot, changed, edit, measure)
        if args.guest_limit: verify_guest_limit(page, remote, row, snapshot)
        row.update(passed=True, layout=layout, audit=summary)
    except Exception as error:
        row.update(passed=False, failure=str(error), traceback=traceback.format_exc(), status=page.locator('#s-status').inner_text())
        row['media_state'] = page.evaluate('()=>arrangementRemoteAudit.mediaState()')
        row['media_events'] = page.evaluate('()=>arrangementRemoteAudit.mediaEvents')
        row['context_state'] = page.evaluate('()=>[...new Set(arrangementAudit.outputs.map(value=>value.context))].map(value=>({state:value.state,time:value.currentTime,rate:value.sampleRate}))')
        row['remote_events'] = page.evaluate('()=>arrangementRemoteAudit.events.slice(-15)')
        diag.capture(args.out / f'{width}-failure.png'); raise
    finally:
        try: row['cleanup'] = page.evaluate('()=>arrangementRemoteAudit.cleanup()')
        except Exception as error: row['cleanup_error'] = str(error)
        context.close()


def run_browser(args, origin, result, sources, requests):
    with sync_playwright() as p:
        launch = {'headless': True}
        if args.browser == 'chromium': launch['ignore_default_args'] = ['--mute-audio']
        result['browser_launch'] = launch
        browser = getattr(p, args.browser).launch(**launch); result['browser'] = f'{args.browser} {browser.version}'
        try:
            for width in [320, 390]: verify(browser, origin, width, args, result, sources, requests)
        finally: browser.close()


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--site', type=Path, required=True); parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--cli', type=Path, required=True); parser.add_argument('--worker', type=Path, required=True)
    parser.add_argument('--ffmpeg', type=Path, default=Path(shutil.which('ffmpeg') or 'ffmpeg'))
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    parser.add_argument('--server-source', type=Path, help='Frozen matching backend source for qualification')
    parser.add_argument('--unavailable', action='store_true', help='Exercise the default-disabled server capability and preserve local work')
    parser.add_argument('--files', action='store_true', help='Also explicitly download WAV/MP3/project and restore the saved project')
    parser.add_argument('--archive', action='store_true', help='Also exercise server-owned ZIP review and atomic source admission')
    parser.add_argument('--archive-mib', type=int, default=0, help='Add this much synthetic unselected filler to the ZIP boundary fixture')
    parser.add_argument('--owner', action='store_true', help='Use the private fixture operator credential for the full multi-job flow; guest quotas stay unchanged')
    parser.add_argument('--guest-limit', action='store_true', help='Exercise the real three-job guest quota and retained applied document')
    args = parser.parse_args()
    args.arrangement = True; args.arrangement_enabled = not args.unavailable; args.out.mkdir(parents=True, exist_ok=True)
    sources = [args.out / 'first.wav', args.out / 'second.wav']; hashes = [fixture(sources[0], 440, 20, 1), fixture(sources[1], 880, 12, 2)]
    if args.archive_mib < 0 or args.archive_mib > 400: parser.error('archive-mib must be between 0 and 400')
    args.archive = args.archive or bool(args.archive_mib)
    if args.guest_limit and args.owner: parser.error('guest-limit requires the normal guest principal')
    if args.archive: args.archive_path = make_archive(args.out, sources, args.ffmpeg, args.archive_mib)
    result = {'scope': 'Actual full Server On UI, authenticated private service and compressed native PCM; synthetic device, no deployment', 'cases': [], 'fixture_sha256': hashes}
    try:
        with RemoteBrowserFixture(args) as owned:
            result['fixture'] = owned.provenance
            result['requests'] = owned.requests
            shutil.copy2(Path(__file__).with_name('studio_capture_processor.js'), owned.site / '__arrangement_capture.js')
            with native_browser_audio_environment(args.out) as audio, native_media_capture(args.out) as monitor:
                result['audio_environment'] = audio
                result['native_capture'] = monitor.evidence; args.monitor = monitor
                run_browser(args, owned.origin, result, sources, owned.requests)
        assert [hashlib.sha256(path.read_bytes()).hexdigest() for path in sources] == hashes
        result['passed'] = True
    finally:
        (args.out / 'results.json').write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8'); print(json.dumps(result, indent=2))


if __name__ == '__main__': main()
