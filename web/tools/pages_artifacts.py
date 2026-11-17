"""Read the published main Pages artifact without forwarding GitHub credentials."""
import hashlib
import json
import os
import re
import stat
import tarfile
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode, urljoin, urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener
import zipfile
from datetime import datetime, timezone

REPOSITORY = 'slimedragonair/SlimeyBeats-EZ-Master'
PUBLISH_ROOT = 'https://slimedragonair.github.io/SlimeyBeats-EZ-Master/'
MAX_BYTES = 256 * 1024 * 1024
MAX_FILES = 10000
SHA = re.compile(r'[0-9a-f]{40}')
MAIN_WORKFLOWS = {'web.yml': {'push', 'workflow_dispatch'},
                  'browser-publication-resume.yml': {'workflow_dispatch'}}


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def copy_bounded(source, destination, limit=MAX_BYTES):
    total = 0
    while block := source.read(min(1024 * 1024, limit - total + 1)):
        total += len(block)
        if total > limit:
            raise ValueError('Response or file exceeds byte limit')
        destination.write(block)
    return total


class GitHubClient:
    def __init__(self, repository=REPOSITORY, token=None, opener=None):
        if repository != REPOSITORY:
            raise ValueError('Repository is not the authorized repository')
        self.token = token if token is not None else os.environ.get('GH_TOKEN', '')
        if not self.token:
            raise ValueError('GH_TOKEN is required for artifact download')
        self.opener = opener or build_opener(NoRedirect())
        self.base = f'https://api.github.com/repos/{repository}/'

    def _open(self, url, authenticated=False, headers=None):
        parsed = urlsplit(url)
        if (parsed.scheme != 'https' or not parsed.hostname or parsed.username
                or parsed.password or parsed.port not in (None, 443)
                or (authenticated and not url.startswith(self.base))):
            raise ValueError('Unsafe HTTP target')
        request_headers = {'User-Agent': 'SlimeyBeats-Pages-composer', **(headers or {})}
        if authenticated:
            request_headers.update(Authorization=f'Bearer {self.token}',
                                   Accept='application/vnd.github+json',
                                   **{'X-GitHub-Api-Version': '2022-11-28'})
        try:
            return self.opener.open(Request(url, headers=request_headers), timeout=60)
        except HTTPError as error:
            if error.code in (301, 302, 303, 307, 308):
                return error
            raise ValueError(f'HTTP request failed (status {error.code})') from None
        except (URLError, OSError):
            raise ValueError('HTTP request failed') from None

    def api(self, path):
        with self._open(self.base + path, True) as response:
            if response.status != 200:
                raise ValueError('Unexpected GitHub API redirect or status')
            data = response.read(2 * 1024 * 1024 + 1)
            if len(data) > 2 * 1024 * 1024:
                raise ValueError('GitHub API response exceeds byte limit')
            return json.loads(data)

    def download(self, artifact_id, destination):
        url = self.base + f'actions/artifacts/{int(artifact_id)}/zip'
        for attempt in range(6):
            # Only the original API request receives credentials, never a redirect.
            with self._open(url, attempt == 0) as response:
                if response.status == 200:
                    return copy_bounded(response, destination)
                location = response.headers.get('Location')
                if response.status not in (301, 302, 303, 307, 308) or not location:
                    raise ValueError('Unexpected artifact download status')
                url = urljoin(url, location)
        raise ValueError('Too many artifact download redirects')

    def published_index(self):
        with self._open(PUBLISH_ROOT + 'index.html', headers={
                'Cache-Control': 'no-cache, no-store', 'Pragma': 'no-cache',
                'Accept-Encoding': 'identity'}) as response:
            if response.status != 200:
                raise ValueError('Published root index could not be verified')
            data = response.read(8 * 1024 * 1024 + 1)
            if len(data) > 8 * 1024 * 1024:
                raise ValueError('Published root index exceeds byte limit')
            return data


def paginated(client, path, key):
    for page in range(1, 11):
        data = client.api(path + ('&' if '?' in path else '?') +
                          urlencode({'per_page': 100, 'page': page}))[key]
        yield from data
        if len(data) < 100:
            return
    raise ValueError('GitHub pagination safety limit reached')


def assert_main_idle(client):
    for workflow in MAIN_WORKFLOWS:
        for status in ('queued', 'in_progress', 'waiting', 'pending', 'requested'):
            runs = client.api(f'actions/workflows/{workflow}/runs?' + urlencode({
                'branch': 'main', 'status': status, 'per_page': 1}))['workflow_runs']
            if runs:
                raise ValueError('Main publication workflow is active; retry after it finishes')


def main_run_created(run, workflow):
    """Return a sortable creation time only for an explicitly trusted main run."""
    if not isinstance(run, dict):
        return None
    repository, head = run.get('repository', {}), run.get('head_repository', {})
    if not isinstance(repository, dict) or not isinstance(head, dict):
        return None
    paths = {f'.github/workflows/{workflow}' + suffix for suffix in ('', '@main', '@refs/heads/main')}
    if (run.get('path') not in paths or run.get('event') not in MAIN_WORKFLOWS[workflow]
            or run.get('head_branch') != 'main' or run.get('status') != 'completed'
            or run.get('conclusion') != 'success' or type(run.get('id')) is not int or run['id'] <= 0
            or not isinstance(run.get('head_sha'), str) or not SHA.fullmatch(run['head_sha'])
            or repository.get('full_name') != REPOSITORY or head.get('full_name') != REPOSITORY
            or type(repository.get('id')) is not int or repository['id'] <= 0
            or type(head.get('id')) is not int
            or repository['id'] != head.get('id')):
        return None
    try:
        created = datetime.fromisoformat(run['created_at'].replace('Z', '+00:00'))
        return created.astimezone(timezone.utc) if created.tzinfo is not None else None
    except (AttributeError, KeyError, TypeError, ValueError):
        return None


def main_run_published(client, run, workflow):
    """Ignore scope skips only when both publication jobs prove they never ran."""
    attempt = run.get('run_attempt')
    if type(attempt) is not int or attempt <= 0:
        raise ValueError('Main publication run has no valid attempt identity')
    jobs = list(paginated(client, f"actions/runs/{run['id']}/attempts/{attempt}/jobs", 'jobs'))
    conclusions = []
    for name in ('browser', 'deploy'):
        matches = [job for job in jobs if isinstance(job, dict) and job.get('name') == name]
        if (len(matches) != 1 or matches[0].get('run_id') != run['id']
                or matches[0].get('head_sha') != run['head_sha']
                or matches[0].get('status') != 'completed'):
            raise ValueError('Main publication job identity or completion could not be verified')
        conclusions.append(matches[0].get('conclusion'))
    if conclusions == ['success', 'success']:
        return True
    if workflow == 'web.yml' and conclusions == ['skipped', 'skipped']:
        return False
    raise ValueError('Main publication jobs did not both succeed or prove a scope-only skip')


def select_main_artifact(client, allow_expired=False):
    assert_main_idle(client)
    candidates = []
    for workflow in MAIN_WORKFLOWS:
        for run in paginated(client, f'actions/workflows/{workflow}/runs?branch=main&status=success', 'workflow_runs'):
            created = main_run_created(run, workflow)
            if created is not None:
                candidates.append((created, run['id'], workflow, run))
    for _, _, workflow, run in sorted(candidates, key=lambda value: value[:2], reverse=True):
        if main_run_published(client, run, workflow):
            break
    else:
        raise ValueError('No successful trusted main publication workflow exists')
    artifacts = [artifact for artifact in paginated(
        client, f"actions/runs/{run['id']}/artifacts", 'artifacts') if artifact['name'] == 'github-pages']
    if len(artifacts) > 1:
        raise ValueError('Ambiguous main Pages artifact')
    if artifacts:
        if not 0 < artifacts[0]['size_in_bytes'] <= MAX_BYTES:
            raise ValueError('Pages artifact exceeds byte limit')
        if allow_expired or not artifacts[0]['expired']:
            return run, artifacts[0]
    if allow_expired:
        return run, artifacts[0] if artifacts else None
    raise ValueError('Latest successful main has no retained Pages artifact')


def assert_main_source(client, run_id, commit, artifact_id):
    run, artifact = select_main_artifact(client, allow_expired=True)
    if ((run['id'], run['head_sha']) != (run_id, commit)
            or (artifact is not None and artifact['id'] != artifact_id)):
        raise ValueError('Selected main artifact changed; recompose the preview')
    return run


def safe_parts(name):
    if not name or name.startswith('/'):
        raise ValueError('Unsafe absolute or empty site path')
    name = name.removeprefix('./').removesuffix('/')
    if name in ('', '.'):
        return ()
    parts = tuple(name.split('/'))
    reserved = {'con', 'prn', 'aux', 'nul'} | {f'{p}{n}' for p in ('com', 'lpt') for n in range(1, 10)}
    if any(not part or part in ('.', '..') or part[-1:] in (' ', '.')
           or any(ord(char) < 32 or char in '\\:<>"|?*' for char in part)
           or part.split('.')[0].casefold() in reserved for part in parts):
        raise ValueError('Unsafe or nonportable site path')
    return parts


def extract_main(archive, destination):
    """Stream regular files only; the old preview subtree is validated but omitted."""
    manifest, seen, total = {}, set(), 0
    with zipfile.ZipFile(archive) as zipped:
        entries = zipped.infolist()
        if (len(entries) != 1 or entries[0].filename != 'artifact.tar'
                or entries[0].file_size > MAX_BYTES
                or stat.S_IFMT(entries[0].external_attr >> 16) not in (0, stat.S_IFREG)):
            raise ValueError('Expected one bounded regular artifact.tar ZIP entry')
        with zipped.open(entries[0]) as stream, tarfile.open(fileobj=stream, mode='r|') as tar:
            for member in tar:
                parts = safe_parts(member.name)
                key = '/'.join(parts).casefold()
                total += member.size
                if (key in seen or len(seen) >= MAX_FILES or total > MAX_BYTES
                        or member.size < 0 or member.sparse is not None
                        or not (member.isfile() or member.isdir())
                        or (not parts and not member.isdir())
                        or (parts and parts[0].casefold() == 'current-work'
                            and parts[0] != 'current-work')):
                    raise ValueError('Unsafe, duplicate or oversized tar member')
                seen.add(key)
                if not parts or parts[0] == 'current-work':
                    continue
                target = destination.joinpath(*parts)
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    with tar.extractfile(member) as source, target.open('xb') as output:
                        if copy_bounded(source, output, member.size) != member.size:
                            raise ValueError('Tar member ended before its declared size')
                    with target.open('rb') as source:
                        manifest['/'.join(parts)] = hashlib.file_digest(source, 'sha256').hexdigest()
    if 'index.html' not in manifest:
        raise ValueError('Main Pages artifact has no root index.html')
    return manifest
