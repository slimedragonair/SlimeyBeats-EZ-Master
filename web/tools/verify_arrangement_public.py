"""Focused published HTTPS acceptance; ordinary guests and witnessed projects only.

Default: one Chromium 390px guest workflow (six heavy jobs), then one WebKit
390px imported 1.25s clip and 20ms suffix (zero heavy jobs). --plan is offline.
Launch only after the owner approves deployment/publication and API probes finish.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import time
from types import SimpleNamespace
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import Request, urlopen

PAGE_ROOT = 'https://slimedragonair.github.io/SlimeyBeats-EZ-Master/'
API_ORIGIN = 'https://slimeybeats-preview.tailf8de2d.ts.net'
ADMISSION = {'review_archive', 'commit_import'}
HEAVY = {'open_project', 'save_project', 'suggest_eq', 'export'}


def checked_commit(value):
    if not re.fullmatch('[a-fA-F0-9]{40}', value): raise argparse.ArgumentTypeError('Use a full Git commit SHA')
    return value.lower()


def checked_url(value):
    if value not in [PAGE_ROOT, PAGE_ROOT + 'current-work/']:
        raise argparse.ArgumentTypeError('Use the exact authorized HTTPS site or current-work URL')
    return value


class JobBudget:
    def __init__(self, maximum=14): self.maximum, self.attempts = maximum, []
    def reserve(self, operation):
        if operation not in HEAVY | ADMISSION: raise ValueError('Unexpected arrangement processing operation')
        if operation in HEAVY and self.used >= self.maximum: raise ValueError('Public heavy-job attempt budget exhausted')
        self.attempts.append(operation)
        return self.used
    @property
    def used(self): return sum(operation in HEAVY for operation in self.attempts)


def publication(metadata, expected):
    if (not isinstance(metadata, dict) or metadata.get('branch') != 'current-work' or
            not re.fullmatch('[a-f0-9]{40}', str(metadata.get('commit', ''))) or
            metadata.get('commit') != expected or not re.fullmatch('[a-f0-9]{16}', str(metadata.get('runtime_version', '')))):
        raise ValueError('Publication metadata does not match the authorized commit/runtime')
    return metadata


def same_runtime(metadata, initial):
    value = publication(metadata, metadata.get('commit') if isinstance(metadata, dict) else '')
    if value['runtime_version'] != initial['runtime_version']: raise ValueError('Publication runtime changed during acceptance')
    return value


def await_publication(args):
    deadline = time.monotonic() + args.wait_seconds; last = 'not published'
    while True:
        try:
            request = Request(args.url + 'preview-build.json?acceptance=' + args.commit, headers={'Cache-Control': 'no-cache'})
            with urlopen(request, timeout=15) as response:
                data = response.read(256 * 1024 + 1)
            if len(data) > 256 * 1024: raise ValueError('Publication metadata exceeds its byte budget')
            return publication(json.loads(data), args.commit)
        except HTTPError as error:
            if error.code != 404: raise
            last = 'HTTP 404'
        except ValueError as error: last = str(error)
        if time.monotonic() >= deadline: raise RuntimeError('Expected publication not ready: ' + last)
        time.sleep(3)


def configure(page, row, args, owned, budget, expected_heavy):
    tools = Path(__file__).parent
    row['public_responses'] = []; row['public_downloads'] = []
    page.expose_binding('reservePublicJob', lambda source, operation: budget.reserve(operation))
    page.expose_binding('reportPublicAudit', lambda source, value: row.update(public=value))
    config = {'api': API_ORIGIN, 'expectedHeavy': expected_heavy}
    page.add_init_script(script='window.arrangementPublicConfig=' + json.dumps(config) + ';\n' +
                         (tools / 'arrangement_public_audit.js').read_text(encoding='utf-8'))
    def response(value):
        parsed = urlsplit(value.url); request = value.request
        origin = parsed.scheme + '://' + parsed.netloc
        if origin != API_ORIGIN: return
        headers = value.all_headers()
        record = {'origin': origin, 'path': parsed.path, 'method': request.method, 'status': value.status,
                  'allow_origin': headers.get('access-control-allow-origin'),
                  'authorization_present': 'authorization' in request.all_headers()}
        owned.requests.append(record); row['public_responses'].append(record)
    page.on('response', response)
    page.on('download', lambda value: row['public_downloads'].append({'url': value.url, 'name': value.suggested_filename}))


def ready(page, row, args):
    assert page.url == args.url and page.evaluate('()=>isSecureContext')
    value = args.metadata if args.root_smoke else page.evaluate('''async()=>{
        const response=await fetch(new URL('preview-build.json',location.href),{cache:'no-store',credentials:'omit',redirect:'error'});
        if(!response.ok)throw new Error('Preview identity HTTP '+response.status);return response.json();}''')
    same_runtime(value, args.metadata)
    versions = page.evaluate('''()=>Array.from(document.querySelectorAll('script[src],link[rel="stylesheet"][href]'),
        element=>new URL(element.src||element.href).searchParams.get('v'))''')
    assert versions and all(version == value['runtime_version'] for version in versions), versions
    row['publication'] = value
    row['initial_publication_commit'] = args.metadata['commit']
    if page.locator('#learning-dialog[open]').is_visible(): page.locator('#tutorial-decline').click()
    page.evaluate('()=>arrangementPublicAudit.unlock()')


def validate_case(row, url):
    assert row['passed'] and not row['errors'], row.get('failure')
    audit = row['public']; assert not audit['violations'] and audit['sessions'] == 1
    assert len(audit['created']) == 1 and {item['id'] for item in audit['cleanup']} == set(audit['created'])
    assert all(item['status'] in [200, 204, 404] for item in audit['cleanup'])
    cors = [item for item in row['public_responses'] if item['authorization_present'] and item['status'] < 400]
    assert cors and any(item['allow_origin'] == 'https://slimedragonair.github.io' for item in cors), 'Missing actual authenticated HTTPS CORS evidence'
    worker = urlsplit(audit['service_worker'] or '')
    assert worker.scheme == 'https' and worker.netloc == 'slimedragonair.github.io'
    prefix = urlsplit(url).path
    assert worker.path == prefix + 'remote-media-worker.js'
    assert audit['media_sources'], 'No native media Play was observed'
    for item in audit['media_sources']:
        assert item['origin'] == 'https://slimedragonair.github.io' and not item['query'], item
        assert item['protocol'] == 'blob:' or (item['protocol'] == 'https:' and item['path'].startswith(prefix + 'remote-media/')), item
    assert audit['bridge_paths'] or any(item['protocol'] == 'https:' for item in audit['media_sources'])
    for item in row['public_downloads']:
        parsed = urlsplit(item['url'])
        assert parsed.scheme == 'https' and parsed.netloc == 'slimedragonair.github.io'
        assert parsed.path.startswith(prefix + 'remote-media/') and not parsed.query


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', type=checked_url, default=PAGE_ROOT + 'current-work/')
    parser.add_argument('--commit', type=checked_commit, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--ffmpeg', type=Path, default=Path(shutil.which('ffmpeg') or 'ffmpeg'))
    parser.add_argument('--wait-seconds', type=int, default=300)
    parser.add_argument('--plan', action='store_true', help='Print the offline plan; create no guest session or browser')
    parser.add_argument('--root-smoke', type=Path, metavar='PASSED_PREVIEW_RECEIPT',
                        help='After main publication: same-runtime WebKit bridge/20ms check, zero heavy jobs')
    parser.add_argument('--short-only', action='store_true', help='Focused WebKit suffix diagnostics; zero heavy jobs')
    parser.add_argument('--include-full-clip', action='store_true',
                        help='Also verify gap-free 1.25s playback before the 20ms suffix; zero additional heavy jobs')
    args = parser.parse_args()
    if args.root_smoke: args.url = PAGE_ROOT
    elif args.url == PAGE_ROOT: parser.error('Root acceptance requires a passed --root-smoke preview receipt')
    plan = {'url': args.url, 'expected_commit': args.commit, 'api_origin': API_ORIGIN,
            'width': 390, 'browsers': ['chromium full guest workflow', 'webkit 1.25s source / 20ms suffix'],
            'planned_heavy_jobs': 6, 'maximum_heavy_attempts': 14,
            'cleanup': 'Only witnessed POST /v2/projects HTTP 201 IDs with their original ordinary guest',
            'public_execution': 'Owner deployment/publication GO required; wait for other API acceptance to finish'}
    if args.root_smoke or args.short_only: plan.update(browsers=['webkit bridge / 20ms suffix'], planned_heavy_jobs=0)
    plan['full_clip_prefill_control'] = args.include_full_clip
    if args.plan: print(json.dumps(plan, indent=2)); return
    if args.wait_seconds < 0: parser.error('wait-seconds must be nonnegative')
    args.out.mkdir(parents=True, exist_ok=False)
    budget = JobBudget(); result = {**plan, 'passed': False, 'cases': []}
    try:
        if args.root_smoke:
            preview = json.loads(args.root_smoke.read_text(encoding='utf-8'))
            if not preview.get('passed') or preview.get('api_origin') != API_ORIGIN: raise ValueError('Require a passed public preview receipt')
            args.metadata = publication(preview.get('metadata'), args.commit)
            result['root_scope'] = 'Same published runtime as passed preview; main Git provenance belongs to the publication receipt'
        else: args.metadata = await_publication(args)
        result['metadata'] = args.metadata
        from playwright.sync_api import sync_playwright
        from native_media_capture import native_browser_audio_environment, native_media_capture
        from verify_arrangement_guest_ui import verify as guest
        from verify_arrangement_short_ui import verify as short, marked_source
        from verify_arrangement_browser import fixture
        from arrangement_remote_archive import make_archive
        sources = [args.out / 'first.wav', args.out / 'second.wav']
        hashes = [fixture(sources[0], 440, 20, 1), fixture(sources[1], 880, 12, 2)]
        args.archive_path = make_archive(args.out, sources, args.ffmpeg, 0)
        marked = args.out / 'markers.wav'; markers, digest = marked_source(marked, 60000)
        args.migration_only = args.cleanup_control = args.large_budget = False
        args.suffix_only = not args.include_full_clip
        owned = SimpleNamespace(origin=args.url, requests=[])
        result['requests'] = owned.requests
        with native_browser_audio_environment(args.out) as audio, native_media_capture(args.out) as monitor, sync_playwright() as p:
            result.update(audio_environment=audio, native_capture=monitor.evidence); args.monitor = monitor
            for name, expected in ([('webkit', 0)] if args.root_smoke or args.short_only else [('chromium', 6), ('webkit', 0)]):
                args.browser = name
                args.page_setup = lambda page, row, expected=expected: configure(page, row, args, owned, budget, expected)
                args.page_ready = lambda page, row: ready(page, row, args)
                options = {'ignore_default_args': ['--mute-audio']} if name == 'chromium' else {}
                browser = getattr(p, name).launch(headless=True, **options)
                try:
                    if name == 'chromium': guest(browser, owned, args, result, sources, 390)
                    else: short(browser, owned, args, monitor, result, marked, 60000, markers, 390)
                    validate_case(result['cases'][-1], args.url)
                    if name == 'webkit' and args.include_full_clip:
                        media = result['cases'][-1]['public']['media_sources'][0]
                        assert media['ready'] >= 1 and media['buffered'], media
                        assert media['buffered'][-1][1] - media['buffered'][0][0] >= .5, media
                finally: browser.close()
        assert [hashlib.sha256(path.read_bytes()).hexdigest() for path in sources] == hashes
        assert hashlib.sha256(marked.read_bytes()).hexdigest() == digest
        assert budget.used <= 14; result['passed'] = True
    finally:
        result.update(heavy_attempts=budget.used, processing_attempts=budget.attempts)
        (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
        print(json.dumps({'passed': result['passed'], 'heavy_attempts': budget.used, 'out': str(args.out)}))


if __name__ == '__main__': main()
