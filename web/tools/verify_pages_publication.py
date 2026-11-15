"""Check the public preview identity and stable site bytes after Pages deployment."""
import argparse
import hashlib
import json
from pathlib import Path
import time
from urllib.error import URLError
from urllib.parse import quote
from urllib.request import Request, urlopen

from pages_artifacts import PUBLISH_ROOT, REPOSITORY, SHA

MAX_FILE_BYTES = 32 * 1024 * 1024
STABLE_FILES = ('index.html', 'app.js', 'styles.css', 'responsive.css', 'pkg/dsp_bg.wasm')
PREVIEW_FILES = ('index.html', 'bootstrap.js', 'preview-info.js', 'pkg/dsp_bg.wasm')


def public_bytes(relative, commit):
    url = PUBLISH_ROOT + quote(relative, safe='/') + '?publication=' + commit
    request = Request(url, headers={'User-Agent': 'SlimeyBeats-publication-check',
        'Cache-Control': 'no-cache, no-store', 'Accept-Encoding': 'identity'})
    with urlopen(request, timeout=20) as response:
        if response.status != 200 or not response.url.startswith(PUBLISH_ROOT):
            raise ValueError('Unexpected public site response')
        data = response.read(MAX_FILE_BYTES + 1)
        if len(data) > MAX_FILE_BYTES:
            raise ValueError('Public file exceeds verification budget')
        return data


def check_publication(report, fetch=public_bytes):
    expected = report['preview']
    if report['repository'] != REPOSITORY or not SHA.fullmatch(expected['commit']):
        raise ValueError('Invalid publication identity')
    commit = expected['commit']
    live = json.loads(fetch('current-work/preview-build.json', commit))
    for key in ('branch', 'commit', 'main_commit', 'main_run_id', 'runtime_version'):
        if live.get(key) != expected.get(key):
            raise ValueError('Public preview identity is not the expected build')
    for prefix, names, manifest in [('', STABLE_FILES, report['main_files']),
            ('current-work/', PREVIEW_FILES, report['preview_files'])]:
        for name in names:
            relative = prefix + name
            if hashlib.sha256(fetch(relative, commit)).hexdigest() != manifest[relative]:
                raise ValueError('Public file does not match its verified publication: ' + relative)
    return {'passed': True, 'commit': commit, 'runtime_version': expected['runtime_version'],
        'stable_files_verified': len(STABLE_FILES), 'preview_files_verified': len(PREVIEW_FILES),
        'preview_url': PUBLISH_ROOT + 'current-work/'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--timeout', type=float, default=180)
    args = parser.parse_args()
    if not 1 <= args.timeout <= 300:
        parser.error('--timeout must be between 1 and 300 seconds')
    report = json.loads(args.report.read_text(encoding='utf-8'))
    deadline = time.monotonic() + args.timeout
    while True:
        try:
            print(json.dumps(check_publication(report), indent=2))
            return
        except (URLError, OSError, ValueError, KeyError) as error:
            if time.monotonic() >= deadline:
                raise SystemExit('Public verification failed: ' + str(error)) from None
            print('Waiting for the expected Pages publication…', flush=True)
            time.sleep(5)


if __name__ == '__main__':
    main()
