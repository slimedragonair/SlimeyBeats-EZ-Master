"""Fail-closed reuse of the qualified prefix of one trusted main browser run.

baseline checks GitHub evidence and source; qualify binds downloaded reports to
the rebuilt site; guard repeats source/main/runtime checks immediately to publish.
Only GITHUB_TOKEN is used, for read-only API requests; never log credentials.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

REPOSITORY = 'slimedragonair/SlimeyBeats-EZ-Master'
BASELINE_HEAD = 'cb521041440b8d665f95dd9d08c1851b406ec157'
WORKFLOW = '.github/workflows/web.yml'
ARTIFACT = 'SlimeyBeats-browser-validation'
PRIOR = '''Set up job
Run actions/checkout@v6
Set up Rust and WebAssembly
Install matching wasm-bindgen CLI
Check binding crate
Build static site and license inventory
Install browser test tools
Verify recovery and private reporting in three engines
Verify MP3 timing against independent decoded PCM
Verify long WAV and MP3 imports in three engines
Verify Chromium
Build fixed hybrid workers and verify streaming Studio in three engines
Verify Firefox
Verify WebKit
Verify bounded live EQ and portable exports in three engines
Verify repeated track replacement and export
Verify Studio editing in three engines'''.splitlines()
REMAINING = '''Verify browser stem timeline and shared time edits
Verify shared Issue Detector and Stem Doctor in three engines
Verify Studio replacement and decoded input bounds
Verify audible live stems and exclusive playback in three engines
Verify learning, persistent choices and compact layouts'''.splitlines()
ALLOWED = {
    'web/tools/region_edit_ui_checks.py', 'web/tools/test_region_edit_download.py',
    '.github/ci/browser_resume.py', '.github/ci/test_browser_resume.py',
    '.github/workflows/browser-publication-resume.yml',
    '.github/ci/check_scope.py', '.github/ci/test_check_scope.py',
    '.github/workflows/change-scope.yml', 'README.md',
    'web/tools/pages_artifacts.py', 'web/tools/pages_baseline.py',
    'web/tools/compose_pages_preview.py', 'web/tools/test_pages_preview.py',
    'web/tools/test_pages_baseline.py', 'web/tools/test_pages_artifacts.py',
    '.github/workflows/current-work-preview.yml',
    'mcp/tools/verify_optional_tools.py', 'mcp/tools/verify_connect_config.ps1',
    '.github/workflows/ci.yml',
}
JSON_LIMIT = 4 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def api(path):
    token = os.environ.get('GITHUB_TOKEN')
    require(token, 'GITHUB_TOKEN with actions:read and contents:read is required')
    request = Request(f'https://api.github.com/repos/{REPOSITORY}/{path}', headers={
        'Authorization': f'Bearer {token}', 'Accept': 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28', 'User-Agent': 'browser-publication-resume'})
    try:
        with urlopen(request, timeout=30) as response:
            data = response.read(JSON_LIMIT + 1)
    except HTTPError as error:
        raise ValueError(f'GitHub API rejected read-only metadata request (HTTP {error.code})') from None
    except URLError:
        raise ValueError('GitHub API metadata request failed') from None
    require(len(data) <= JSON_LIMIT, 'GitHub metadata exceeds size budget')
    return json.loads(data)


def git(repo, *args):
    return subprocess.run(['git', '-C', str(repo), *args], check=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout.decode().strip()


def allowed(path):
    return path in ALLOWED or (path.startswith('docs/') and path.endswith('.md')
                              and '..' not in path.split('/'))


def source_guard(repo, head, fetch=api):
    require(re.fullmatch('[a-f0-9]{40}', head or ''), 'Expected full dispatch commit SHA')
    require(git(repo, 'rev-parse', 'HEAD') == head, 'Checkout differs from dispatch commit')
    git(repo, 'merge-base', '--is-ancestor', BASELINE_HEAD, head)
    changed = git(repo, 'diff', '--no-ext-diff', '--no-textconv', '--no-renames',
                  '--name-only', '-z', BASELINE_HEAD, head, '--').split('\0')
    dirty = git(repo, 'status', '--porcelain', '--untracked-files=normal')
    require(not dirty, 'Checkout has local changes; refusing to qualify different bytes')
    changed = [path for path in changed if path]
    require(all(allowed(path) for path in changed), 'Application inputs changed; full web.yml validation required: '
            + ', '.join(path for path in changed if not allowed(path)))
    require(fetch('git/ref/heads/main')['object']['sha'] == head,
            'Main advanced since dispatch; refusing stale publication')
    return changed


def baseline(run_id, repo, head, fetch=api):
    require(type(run_id) is int and run_id > 0, 'Expected positive baseline run ID')
    run = fetch(f'actions/runs/{run_id}')
    require(run['id'] == run_id and run['path'] == WORKFLOW and run['event'] == 'push'
            and run['head_branch'] == 'main' and run['head_sha'] == BASELINE_HEAD
            and run['status'] == 'completed' and run['conclusion'] == 'failure',
            'Baseline must be the trusted failed web.yml main push at cb521041440b')
    require(run['repository']['full_name'] == REPOSITORY
            and run['head_repository']['full_name'] == REPOSITORY
            and run['repository']['id'] == run['head_repository']['id'], 'Foreign baseline repository')
    workflow = fetch('actions/workflows/web.yml')
    require(workflow['id'] == run['workflow_id'] and workflow['path'] == WORKFLOW, 'Wrong baseline workflow')
    jobs = fetch(f'actions/runs/{run_id}/attempts/{run["run_attempt"]}/jobs?per_page=100')
    require(jobs['total_count'] == len(jobs['jobs']) <= 100, 'Incomplete baseline jobs metadata')
    matches = [job for job in jobs['jobs'] if job['name'] == 'browser']
    require(len(matches) == 1, 'Expected one baseline browser job')
    job = matches[0]
    require(job['run_id'] == run_id and job['head_sha'] == BASELINE_HEAD
            and job['status'] == 'completed' and job['conclusion'] == 'failure', 'Wrong baseline browser job')
    steps = sorted(job['steps'], key=lambda step: step['number'])
    expected = PRIOR + REMAINING
    require([step['name'] for step in steps[:len(expected)]] == expected, 'Missing/reordered browser validation steps')
    for index, step in enumerate(steps[:len(expected)]):
        conclusion = 'success' if index < len(PRIOR) else 'failure' if index == len(PRIOR) else 'skipped'
        require(step['status'] == 'completed' and step['conclusion'] == conclusion,
                'Unexpected baseline qualification: ' + step['name'])
    require([s['name'] for s in steps if s['conclusion'] == 'failure'] == REMAINING[:1],
            'Baseline failed outside the expected timeline step')
    listing = fetch(f'actions/runs/{run_id}/artifacts?per_page=100')
    require(listing['total_count'] == len(listing['artifacts']) <= 100, 'Incomplete artifact metadata')
    artifacts = [a for a in listing['artifacts'] if a['name'] == ARTIFACT]
    require(len(artifacts) == 1, 'Expected one named baseline validation artifact')
    artifact = artifacts[0]
    require(not artifact['expired'] and 0 < artifact['size_in_bytes'] <= 512 * 1024 * 1024
            and re.fullmatch('sha256:[a-f0-9]{64}', artifact.get('digest', '')),
            'Baseline artifact expired, unbounded, or missing digest')
    origin = artifact['workflow_run']
    require(origin['id'] == run_id and origin['head_sha'] == BASELINE_HEAD
            and origin['head_branch'] == 'main' and origin['repository_id'] == run['repository']['id']
            and origin['head_repository_id'] == run['repository']['id'], 'Foreign baseline artifact')
    return dict(repository=REPOSITORY, head=head, baseline_head=BASELINE_HEAD,
                baseline_run=run_id, baseline_attempt=run['run_attempt'], baseline_job=job['id'],
                artifact_id=artifact['id'], artifact_digest=artifact['digest'],
                changed_paths=source_guard(repo, head, fetch), reused_successful_steps=PRIOR,
                rerun_required_steps=REMAINING, baseline_steps=steps)


def report_runtime(root):
    versions, reports = set(), {}
    for engine in ('chromium', 'firefox', 'webkit'):
        path = root / f'bootstrap-{engine}' / 'bootstrap-verification.json'
        require(path.is_file() and not path.is_symlink() and path.stat().st_size <= JSON_LIMIT,
                'Missing/unbounded passed baseline report: ' + str(path))
        raw = path.read_bytes()
        report = json.loads(raw)
        version = report.get('runtime_version', '')
        require(report.get('success') is True and str(report.get('browser', '')).startswith(engine + ' ')
                and re.fullmatch('[a-f0-9]{16}', version), 'Unqualified baseline runtime report')
        versions.add(version)
        reports[path.relative_to(root).as_posix()] = hashlib.sha256(raw).hexdigest()
    require(len(versions) == 1, 'Passed baseline reports disagree on runtime')
    return versions.pop(), reports


def site_identity(root):
    text = (root / 'index.html').read_text(encoding='utf-8')
    assets = re.findall(r'''(?:src|href)=["']([^"']+\.(?:js|css)(?:\?[^"']*)?)["']''', text)
    versions = [re.fullmatch(r'[^?]+\?v=([a-f0-9]{16})', asset) for asset in assets]
    require(versions and all(versions) and len({v[1] for v in versions}) == 1,
            'Built site has missing or mixed runtime versions')
    digest = hashlib.sha256()
    for path in sorted(root.rglob('*')):
        require(not path.is_symlink(), 'Symlink in rebuilt site')
        if path.is_file():
            digest.update(path.relative_to(root).as_posix().encode() + b'\0')
            with path.open('rb') as source:
                digest.update(hashlib.file_digest(source, 'sha256').digest())
    return versions[0][1], digest.hexdigest()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['baseline', 'qualify', 'guard'])
    parser.add_argument('--baseline-run', type=int, default=44029858651)
    parser.add_argument('--repo', type=Path, default=Path.cwd())
    parser.add_argument('--receipt', type=Path, required=True)
    parser.add_argument('--reports', type=Path)
    parser.add_argument('--site', type=Path)
    args = parser.parse_args(argv)
    require(os.environ.get('GITHUB_REPOSITORY') == REPOSITORY
            and os.environ.get('GITHUB_REF') == 'refs/heads/main'
            and os.environ.get('GITHUB_EVENT_NAME') == 'workflow_dispatch', 'Manual main dispatch only')
    head = os.environ.get('GITHUB_SHA', '')
    if args.command == 'baseline':
        receipt = baseline(args.baseline_run, args.repo, head)
    else:
        receipt = json.loads(args.receipt.read_text(encoding='utf-8'))
        require(receipt['repository'] == REPOSITORY and receipt['head'] == head
                and receipt['baseline_head'] == BASELINE_HEAD, 'Receipt source identity mismatch')
        require(source_guard(args.repo, head) == receipt['changed_paths'], 'Source diff changed')
        require(args.site, '--site is required')
        runtime, site_hash = site_identity(args.site)
        if args.command == 'qualify':
            require(args.reports, '--reports is required')
            expected, reports = report_runtime(args.reports)
            require(runtime == expected, 'Rebuilt runtime differs from passed baseline; full validation required')
            receipt.update(runtime_version=runtime, site_sha256=site_hash, passed_report_sha256=reports)
        else:
            require(runtime == receipt['runtime_version'] and site_hash == receipt['site_sha256'],
                    'Qualified runtime/site bytes changed before publication')
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
    print(f'{args.command}: source/main guard passed; receipt: {args.receipt}')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, TypeError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit('Browser resume refused: ' + str(error)) from None
