"""Ordinary guest continuation and complete ZIP/editor/delivery workflow."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import time
import traceback
import zipfile

from playwright.sync_api import expect, sync_playwright
from arrangement_remote_archive import make_archive, verify_archive
from arrangement_remote_diagnostics import ArrangementRemoteDiagnostics
from arrangement_remote_migration import prepare_local, assert_migrated, assert_pending
from arrangement_remote_files import measure_delivery
from arrangement_remote_budget import make_budget_sources, add_budget_sources
from browser_audio_environment import browser_audio_environment
from remote_browser_fixture import RemoteBrowserFixture
from verify_arrangement_browser import fixture


def verify(browser, owned, args, result, sources, width):
    initial_request = len(owned.requests)
    context = browser.new_context(viewport={'width': width, 'height': 844}); page = context.new_page()
    row = {'width': width, 'checks': [], 'errors': [], 'downloads': [], 'measurements': {}}; result['cases'].append(row)
    diag = ArrangementRemoteDiagnostics(page, args.browser, row)
    page.add_init_script(path=str(Path(__file__).with_name('arrangement_audio_audit.js')))
    page.add_init_script(path=str(Path(__file__).with_name('arrangement_remote_audit.js')))
    page.on('download', lambda download: row['downloads'].append(download.suggested_filename))
    local = lambda name: page.locator(f'#s-arrangement-page [data-arr="{name}"]')
    a = lambda name: page.locator(f'#s-arrangement-server-page [data-arr="{name}"]')
    snapshot = lambda: page.evaluate('()=>arrangementRemoteAudit.latest')
    def ready(previous):
        page.wait_for_function('rev=>arrangementRemoteAudit.latest?.revision>rev&&!arrangementRemoteAudit.latest.preview.pending', arg=previous, timeout=120000)
        expect(a('add-files')).to_be_enabled(timeout=30000); return snapshot()
    def edit(name):
        previous = snapshot()['revision']; a(name).click(); return ready(previous)
    def measure(name):
        page.wait_for_function('()=>arrangementRemoteAudit.mediaState().some(value=>!value.paused&&value.time>.25)', timeout=15000)
        page.evaluate('()=>arrangementAudit.attach()'); page.evaluate('()=>arrangementAudit.reset(18000)')
        page.wait_for_function('()=>arrangementAudit.frames>=18000', timeout=15000)
        value = page.evaluate('()=>arrangementAudit.measure()'); assert value['peak'] > 1e-5, value
        row['measurements'][name] = value; return value
    def settled(name, predicate):
        started = time.monotonic(); samples = []
        while time.monotonic() - started < 4:
            value = measure(name); samples.append(value)
            if predicate(value):
                value['settle_seconds'] = time.monotonic() - started
                value['settle_samples'] = len(samples)
                return value
            if len(samples) == 1:
                row.setdefault('first_unsettled_capture', {})[name] = page.evaluate('()=>({pcm:arrangementAudit.pcm,media:arrangementRemoteAudit.mediaState(),stream:arrangementRemoteAudit.streams.at(-1)})')
        raise AssertionError({'buffered_preview_did_not_settle_within_seconds': 4, 'samples': samples})
    try:
        page.goto(owned.origin, wait_until='networkidle'); expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=45000)
        page.locator('#tab-studio-tools').click(); expect(page.locator('#s-arrange-entry')).to_be_visible(timeout=30000)
        page.locator('#s-arrange-entry').click(); saved = prepare_local(page, local, sources)
        page.locator('#hybrid-enabled').check(); expect(page.locator('#hybrid-availability')).to_have_text('[Server online]', timeout=30000)
        assert page.evaluate('()=>arrangementRemoteAudit.uploads.length') == 0
        assert not any('/arrangement/streams/' in value['path'] for value in owned.requests[initial_request:])
        row['checks'].append('Server On alone sends no audio, creates no arrangement stream and preserves local applied work and drafts')
        page.locator('#s-arrange-entry').click(); expect(a('workspace')).to_be_visible(timeout=120000)
        expect(a('add-files')).to_be_enabled(timeout=120000)
        current = snapshot(); assert_migrated(page, a, saved, current, row)
        if args.migration_only: row['passed'] = True; return
        worker_mark = page.evaluate('()=>arrangementAudit.actions.length')
        block_mark = page.evaluate('()=>{arrangementRemoteAudit.serverPhase=true;return arrangementAudit.renderBlocks}')
        current = verify_archive(page, a, row, args.archive_path, snapshot, ready, edit, measure)
        lane = next(lane for lane in current['arrangement']['lanes'] if lane['name'] == 'Lead'); clip = lane['clips'][0]
        a('lane-select').select_option(lane['id']); a('clip-select').select_option(clip['id'])
        a('eq-scope').select_option('range'); a('eq-range-target').select_option('clip')
        a('range-start').fill('1.5'); a('range-end').fill('8.5'); a('range-end').press('Tab')
        previous = snapshot()['revision']; a('eq-suggest').click()
        expect(a('eq-apply-proposal')).to_be_visible(timeout=120000); expect(a('eq-suggest')).to_be_enabled(timeout=30000)
        assert snapshot()['revision'] == previous
        row['checks'].append('the same ordinary guest reviews a ZIP and measures an unapplied explicit clip/range proposal')
        a('clip-start').fill('0.5'); a('clip-source').fill('0.25'); a('clip-end').fill('20.25'); current = edit('clip-timing')
        a('range-start').fill('1.5'); a('range-end').fill('8.5'); a('range-end').press('Tab')
        a('eq-band').select_option('2'); a('eq-frequency').fill('440'); a('eq-shape').select_option('bell'); a('eq-q').fill('1'); a('eq-gain').fill('-6'); current = edit('eq-update')
        effect = next(effect for effect in current['arrangement']['eq'] if effect['scope'] == {'kind': 'clip', 'clip_id': clip['id']})
        assert effect['range'] == {'start_frame': 60000, 'end_frame': 396000}
        a('lane-gain').fill('-6'); current = edit('lane-update'); document = current['arrangement']
        page.locator('#s-stop').click(); page.locator('#s-seek').fill('1.6'); page.locator('#s-play').click()
        expect(page.locator('#s-play')).to_have_text('Pause', timeout=45000)
        live = measure('guest scoped PCM'); assert abs(live['first'] / live['second'] - 10 ** (-12/20)) < .025, live
        page.locator('#s-before').click(); expect(page.locator('#s-play')).to_have_text('Pause', timeout=45000)
        before = measure('guest Before last edit'); assert abs(before['first'] / live['first'] - 10 ** (6/20)) < .06, before
        page.locator('#s-after').click(); expect(page.locator('#s-play')).to_have_text('Pause', timeout=45000)
        after = settled('guest B', lambda value: abs(value['first'] / live['first'] - 1) < .04)
        if args.cleanup_control:
            assert not row['errors'], row['errors']
            row['cleanup'] = page.evaluate('()=>arrangementRemoteAudit.cleanup()')
            page.wait_for_function('()=>arrangementRemoteAudit.events.some(value=>value.operation==="stop"&&value.status===404)', timeout=15000)
            row['cleanup_failures'] = page.evaluate('()=>({started:arrangementRemoteAudit.cleanupAt,requests:arrangementRemoteAudit.events.filter(value=>value.status>=400)})')
            assert all(value['operation'] == 'stop' and value['status'] == 404 and value['at'] >= row['cleanup_failures']['started'] for value in row['cleanup_failures']['requests'])
            row.update(passed=True, expected_failure_control=True)
            return
        page.locator('#s-stop').click()
        row['checks'].append('guest precise source-frame EQ and track edits change actual compressed PCM with exclusive running Before last edit/B')
        if not a('file-actions').evaluate('element=>element.open'): a('file-actions').locator('summary').click()
        for format, suffix in [('mp3', 'mp3'), ('wav32f', 'wav')]:
            a('audio-format').select_option(format)
            with page.expect_download(timeout=120000) as pending: a('audio-export').click()
            path = args.out / f'{width}-guest.{suffix}'; pending.value.save_as(str(path))
            expect(a('audio-export')).to_be_enabled(timeout=30000)
            value = measure_delivery(path, format, args.ffmpeg)
            assert abs(value['first'] - .06 * 10 ** (-12/20)) < .0007 and abs(value['second'] - .06) < .0007, value
            assert value['frames'] == 972000 if format == 'wav32f' else 972000 <= value['frames'] <= 984000
            row['measurements'][f'guest {format}'] = value
            assert snapshot()['arrangement'] == document
        with page.expect_download(timeout=120000) as pending: a('project-save').click()
        project = args.out / f'{width}-guest-project.zip'; pending.value.save_as(str(project)); expect(a('project-save')).to_be_enabled(timeout=30000)
        with zipfile.ZipFile(project) as archive:
            manifest = json.loads(archive.read('arrangement.json')); assert manifest['history']['current'] == document
            assert manifest['history']['undo'] and manifest['undo_labels']
            assert all(item['name'] != 'Unapplied track name' for item in document['lanes'])
        row['checks'].append('one ordinary guest exports actual MP3 and WAV, independently decodes applied EQ, then saves its canonical history project')
        a('clip-gain').fill('-1'); current = edit('clip-level'); document = current['arrangement']
        a('audio-format').select_option('mp3')
        with page.expect_download(timeout=120000) as pending: a('audio-export').click()
        tweaked = args.out / f'{width}-guest-tweak.mp3'; pending.value.save_as(str(tweaked)); expect(a('audio-export')).to_be_enabled(timeout=30000)
        value = measure_delivery(tweaked, 'mp3', args.ffmpeg)
        assert abs(value['first'] / row['measurements']['guest mp3']['first'] - 10 ** (-1/20)) < .025 and abs(value['second'] - .06) < .0007, value
        row['checks'].append('a further reasonable clip-level tweak and repeated MP3 export use the same guest without an owner token or a new session')
        if args.large_budget:
            current = add_budget_sources(page, a, row, args.budget_sources, snapshot, edit); document = current['arrangement']
        count = len(row['downloads']); upload_count = page.evaluate('()=>arrangementRemoteAudit.uploads.length')
        request_mark = len(owned.requests)
        page.locator('#hybrid-enabled').uncheck(); expect(local('workspace')).to_be_visible(timeout=15000)
        assert page.evaluate('()=>arrangementAudit.latest.arrangement') == saved['arrangement']; assert_pending(local)
        expect(page.locator('#s-play')).to_have_text('Play'); assert len(row['downloads']) == count
        if args.large_budget: expect(page.locator('#s-status')).to_contain_text('128 MiB local source budget')
        assert not any(value['method'] == 'GET' and ('/artifacts/' in value['path'] or '/assets/' in value['path']) for value in owned.requests[request_mark:])
        page.locator('#hybrid-enabled').check(); expect(page.locator('#hybrid-availability')).to_have_text('[Server online]', timeout=30000)
        page.locator('#s-arrange-entry').click(); expect(a('workspace')).to_be_visible(timeout=30000)
        assert snapshot()['arrangement'] == document and page.evaluate('()=>arrangementRemoteAudit.uploads.length') == upload_count
        expect(page.locator('#s-status')).to_contain_text('Existing server')
        row['checks'].append('Server Off restores untouched local drafts; returning to a populated server retains its edits without another upload or implicit merge')
        audit = page.evaluate('()=>arrangementRemoteAudit.summary()'); assert audit['credentials_memory_only'] and audit['fullMediaReads'] == 0 and audit['largeBlobReads'] == 0 and audit['clientDecodes'] == 0
        assert page.evaluate('()=>arrangementAudit.renderBlocks') == block_mark
        actions = page.evaluate('mark=>arrangementAudit.actions.slice(mark)', worker_mark)
        assert not any(action in ['stage-wav', 'stage-asset', 'inspect-archive', 'extract-archive', 'export-wav', 'master', 'master_preview', 'measure-encoded'] for action in actions), actions
        assert len([value for value in owned.requests if value['path'] == '/v1/session' and value['method'] == 'POST']) == len(result['cases'])
        assert row['downloads'] == ['Studio-arrangement.mp3', 'Studio-arrangement.wav', 'Studio-arrangement.zip', 'Studio-arrangement.mp3']
        assert not row['errors'], row
        a('lanes').scroll_into_view_if_needed(); diag.capture(args.out / f'{width}-guest.png')
        row.update(passed=True, audit=audit, guest_jobs=page.evaluate('()=>arrangementRemoteAudit.jobs'))
    except Exception as error:
        row.update(passed=False, failure=str(error), traceback=traceback.format_exc(), status=page.locator('#s-status').inner_text())
        row['media_state'] = page.evaluate('()=>arrangementRemoteAudit.mediaState()')
        row['remote_events'] = page.evaluate('()=>arrangementRemoteAudit.events.slice(-40)')
        row['failed_requests'] = page.evaluate('()=>arrangementRemoteAudit.events.filter(value=>value.status>=400)')
        row['streams'] = page.evaluate('()=>arrangementRemoteAudit.streams')
        diag.capture(args.out / f'{width}-failure.png'); raise
    finally:
        try:
            if 'cleanup' not in row: row['cleanup'] = page.evaluate('()=>arrangementRemoteAudit.cleanup()')
        except Exception as error: row['cleanup_error'] = str(error)
        context.close()


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--site', type=Path, required=True); parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--cli', type=Path, required=True); parser.add_argument('--worker', type=Path, required=True)
    parser.add_argument('--ffmpeg', type=Path, default=Path(shutil.which('ffmpeg') or 'ffmpeg'))
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    parser.add_argument('--archive-mib', type=int, default=0); parser.add_argument('--migration-only', action='store_true')
    parser.add_argument('--large-budget', action='store_true', help='Also admit real server sources above the local 128 MiB budget and check Server Off before any download')
    parser.add_argument('--cleanup-control', action='store_true', help='Negative fixture control: delete its owned project while playing and attribute the resulting retired Stop 404')
    args = parser.parse_args(); args.out.mkdir(parents=True, exist_ok=True); args.arrangement = True; args.owner = False
    sources = [args.out / 'first.wav', args.out / 'second.wav']; hashes = [fixture(sources[0], 440, 20, 1), fixture(sources[1], 880, 12, 2)]
    args.archive_path = make_archive(args.out, sources, args.ffmpeg, args.archive_mib) if not args.migration_only else None
    if args.large_budget: args.budget_sources = make_budget_sources(args.out)
    result = {'scope': 'Actual ordinary guest, explicit canonical continuation, responsive UI and owned HTTP service; synthetic sources', 'cases': [], 'fixture_sha256': hashes}
    try:
        with RemoteBrowserFixture(args) as owned:
            result['fixture'] = owned.provenance; result['requests'] = owned.requests
            shutil.copy2(Path(__file__).with_name('studio_capture_processor.js'), owned.site / '__arrangement_capture.js')
            with browser_audio_environment(args.out) as audio, sync_playwright() as p:
                result['audio_environment'] = audio
                browser = getattr(p, args.browser).launch(headless=True); result['browser'] = f'{args.browser} {browser.version}'
                try:
                    for width in ([320] if args.cleanup_control else [320, 390]): verify(browser, owned, args, result, sources, width)
                finally: browser.close()
        assert [hashlib.sha256(path.read_bytes()).hexdigest() for path in sources] == hashes
        result['passed'] = True
    finally:
        (args.out / 'results.json').write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')
        print(json.dumps({'passed': result.get('passed', False), 'cases': result['cases'], 'out': str(args.out)}, indent=2))


if __name__ == '__main__': main()
