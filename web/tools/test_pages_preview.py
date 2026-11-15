"""Offline artifact, credential, rollback and composition regression checks."""
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import HTTPError
import zipfile

from compose_pages_preview import compose, verify_report
from pages_artifacts import (GitHubClient, PUBLISH_ROOT, REPOSITORY, copy_bounded,
                             extract_main, select_main_artifact)

COMMIT, MAIN_COMMIT = 'a' * 40, 'b' * 40
VERSION = '0123456789abcdef'
PREVIEW = f'<link href="styles.css?v={VERSION}"><script src="app.js?v={VERSION}"></script>'.encode()
RUN_URL = f'https://github.com/{REPOSITORY}/actions/runs/123'


def archive(members, zip_name='artifact.tar'):
    payload, result = io.BytesIO(), io.BytesIO()
    with tarfile.open(fileobj=payload, mode='w') as tar:
        for name, value in members:
            member = tarfile.TarInfo(name)
            if not isinstance(value, tuple):
                member.size = len(value)
                tar.addfile(member, io.BytesIO(value))
            else:
                member.type, member.linkname = value[0], 'index.html'
                tar.addfile(member)
    with zipfile.ZipFile(result, 'w', zipfile.ZIP_DEFLATED) as zipped:
        zipped.writestr(zip_name, payload.getvalue())
    return result.getvalue()


class FakeClient:
    def __init__(self, payload=None):
        self.payload = payload or archive([('index.html', b'stable'), ('assets/app.js', b'root'),
                                          ('current-work/old.js', b'old preview')])
        self.run = {'id': 42, 'head_sha': MAIN_COMMIT, 'head_branch': 'main',
                    'status': 'completed', 'conclusion': 'success',
                    'head_repository': {'full_name': REPOSITORY}}
        self.artifact = {'id': 84, 'name': 'github-pages', 'expired': False,
                         'size_in_bytes': len(self.payload)}
        self.commit, self.live, self.active = COMMIT, b'stable', None

    def api(self, path):
        if path == 'git/ref/heads/current-work':
            return {'object': {'sha': self.commit}}
        if path.startswith('actions/runs/'):
            return {'artifacts': [self.artifact]}
        return {'workflow_runs': [self.run] if 'status=success' in path
                or (self.active and f'status={self.active}' in path) else []}

    def download(self, artifact_id, destination):
        destination.write(self.payload)

    def published_index(self):
        return self.live


class Response(io.BytesIO):
    def __init__(self, data=b'', status=200, headers=None):
        super().__init__(data)
        self.status, self.headers = status, headers or {}


class PreviewTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.preview, self.out = self.root / 'preview', self.root / 'composed'
        self.preview.mkdir()
        (self.preview / 'index.html').write_bytes(PREVIEW)
        self.client = FakeClient()

    def compose(self, **overrides):
        arguments = dict(preview_site=self.preview, out=self.out, repository=REPOSITORY,
                         commit=COMMIT, run_url=RUN_URL, client=self.client)
        return compose(**(arguments | overrides))

    def test_preserves_main_bytes_replaces_preview_and_reports_exact_hashes(self):
        self.out.mkdir()
        report = self.compose()
        self.assertEqual((self.out / 'index.html').read_bytes(), b'stable')
        self.assertEqual((self.out / 'assets/app.js').read_bytes(), b'root')
        self.assertEqual((self.out / 'current-work/index.html').read_bytes(), PREVIEW)
        self.assertFalse((self.out / 'current-work/old.js').exists())
        metadata = json.loads((self.out / 'current-work/preview-build.json').read_text())
        self.assertEqual(metadata, report['preview'])
        self.assertEqual(metadata['main_commit'], MAIN_COMMIT)
        self.assertEqual(metadata['runtime_version'], VERSION)
        for name, digest in (report['main_files'] | report['preview_files']).items():
            self.assertEqual(digest, hashlib.sha256((self.out / name).read_bytes()).hexdigest())
        self.assertEqual(verify_report(report, REPOSITORY, self.client)['verified'], True)

    def test_rollback_or_changed_preview_head_leaves_output_absent(self):
        for field, value in [('live', b'newer production'), ('commit', 'c' * 40)]:
            with self.subTest(field=field):
                client = FakeClient()
                setattr(client, field, value)
                with self.assertRaises(ValueError):
                    self.compose(client=client)
                self.assertFalse(self.out.exists())

    def test_predeployment_guard_rechecks_identity_main_activity_and_public_bytes(self):
        report = self.compose()
        for field, value in [('live', b'new production'), ('commit', 'c' * 40),
                             ('active', 'queued'), ('active', 'in_progress')]:
            with self.subTest(field=field):
                client = FakeClient()
                setattr(client, field, value)
                with self.assertRaises(ValueError):
                    verify_report(report, REPOSITORY, client)
        self.client.run['id'] += 1
        with self.assertRaises(ValueError):
            verify_report(report, REPOSITORY, self.client)

    def test_rejects_nonempty_output_overlap_invalid_identity_and_stale_metadata(self):
        for overrides in [dict(out=self.preview), dict(out=self.preview / 'nested'),
                          dict(preview_site=self.root, out=self.out), dict(commit='short'),
                          dict(repository='other/repo'), dict(run_url='https://evil.test/123')]:
            with self.subTest(overrides=overrides), self.assertRaises(ValueError):
                self.compose(**overrides)
        self.out.mkdir()
        (self.out / 'keep.txt').write_bytes(b'keep')
        with self.assertRaises(ValueError):
            self.compose()
        self.assertEqual((self.out / 'keep.txt').read_bytes(), b'keep')
        (self.preview / 'preview-build.json').write_bytes(b'{}')
        with self.assertRaises(ValueError):
            self.compose(out=self.root / 'fresh')

    def test_rejects_links_traversal_duplicate_aliases_and_special_tar_members(self):
        for index, (name, value) in enumerate([('../escape', b'x'), ('/escape', b'x'), ('C:/escape', b'x'),
                            ('a\\escape', b'x'), ('CON', b'x'), ('name.', b'x'),
                            ('current-work/link', (tarfile.SYMTYPE,)), ('link', (tarfile.LNKTYPE,)),
                            ('fifo', (tarfile.FIFOTYPE,)), ('device', (tarfile.CHRTYPE,)),
                            ('./index.html', b'duplicate'), ('INDEX.HTML', b'alias')]):
            with self.subTest(name=name, value=value), self.assertRaises(ValueError):
                extract_main(io.BytesIO(archive([('index.html', b'stable'), (name, value)])),
                             self.root / f'bad-{index}')

    def test_rejects_missing_invalid_or_mixed_runtime_versions(self):
        for html in [b'unversioned', b'<script src="app.js"></script>',
                     b'<script src="app.js?v=bad"></script>',
                     PREVIEW.replace(b'app.js?v=' + VERSION.encode(), b'app.js?v=fedcba9876543210')]:
            (self.preview / 'index.html').write_bytes(html)
            with self.subTest(html=html), self.assertRaises(ValueError):
                self.compose()
            self.assertFalse(self.out.exists())

    def test_requires_one_bounded_artifact_tar_and_root_index(self):
        for payload in [archive([('other.html', b'x')]), archive([], 'wrong.tar')]:
            with self.assertRaises(ValueError):
                extract_main(io.BytesIO(payload), self.root / 'unused')
        with patch('pages_artifacts.MAX_BYTES', 1), self.assertRaises(ValueError):
            extract_main(io.BytesIO(self.client.payload), self.root / 'unused')
        with patch('pages_artifacts.MAX_FILES', 1), self.assertRaises(ValueError):
            extract_main(io.BytesIO(self.client.payload), self.root / 'unused')
        with self.assertRaises(ValueError):
            copy_bounded(io.BytesIO(b'1234'), io.BytesIO(), limit=3)

    def test_rejects_active_or_expired_main_artifact(self):
        for status in ('queued', 'in_progress', 'waiting', 'pending', 'requested'):
            self.client.active = status
            with self.subTest(status=status), self.assertRaises(ValueError):
                select_main_artifact(self.client)
        self.client.active, self.client.artifact['expired'] = None, True
        with self.assertRaises(ValueError):
            select_main_artifact(self.client)
        self.assertEqual(select_main_artifact(self.client, allow_expired=True)[0]['id'], 42)
        original = self.client.api
        self.client.api = lambda path: {'workflow_runs': [self.client.run, self.client.run | {'id': 41}]} \
            if 'status=success' in path else ({'artifacts': [self.client.artifact | {'expired': False}]} \
            if path.startswith('actions/runs/41/') else original(path))
        with self.assertRaises(ValueError):
            select_main_artifact(self.client)

    def test_rejects_preview_and_output_symlinks(self):
        link = self.preview / 'linked.html'
        try:
            link.symlink_to(self.preview / 'index.html')
        except OSError as error:
            self.skipTest(f'OS symlink creation unavailable: {error}')
        with self.assertRaises(ValueError):
            self.compose()
        linked_out = self.root / 'linked-output'
        linked_out.symlink_to(self.out, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.compose(out=linked_out)

    def test_signed_redirect_chain_never_receives_github_token(self):
        calls = []
        responses = iter([HTTPError('https://api.github.com/artifact', 302, 'Found',
                                    {'Location': 'https://storage.test/signed'}, io.BytesIO()),
                          Response(status=307, headers={'Location': 'https://cdn.test/final'}),
                          Response(b'zip')])
        class Opener:
            def open(self, request, timeout):
                calls.append(request)
                response = next(responses)
                if isinstance(response, HTTPError):
                    raise response
                return response
        client = GitHubClient(token='secret-fixture', opener=Opener())
        destination = io.BytesIO()
        client.download(84, destination)
        self.assertEqual(destination.getvalue(), b'zip')
        self.assertEqual(calls[0].get_header('Authorization'), 'Bearer secret-fixture')
        self.assertTrue(all(call.get_header('Authorization') is None for call in calls[1:]))
        for url in ('http://storage.test/x', 'https://user:pass@storage.test/x'):
            with self.subTest(url=url), self.assertRaises(ValueError):
                client._open(url)

    def test_public_index_uses_fixed_url_no_cache_and_no_auth(self):
        class Opener:
            def open(inner, request, timeout):
                self.assertEqual(request.full_url, PUBLISH_ROOT + 'index.html')
                self.assertIsNone(request.get_header('Authorization'))
                self.assertIn('no-cache', request.get_header('Cache-control'))
                return Response(b'stable')
        self.assertEqual(GitHubClient(token='secret-fixture', opener=Opener()).published_index(), b'stable')


if __name__ == '__main__':
    unittest.main()
