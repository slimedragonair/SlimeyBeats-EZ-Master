"""Offline artifact, credential, rollback and composition regression checks."""
import hashlib
import io
import json
import re
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import HTTPError
import zipfile

from compose_pages_preview import compose, verify_report
from pages_artifacts import (GitHubClient, PUBLISH_ROOT, REPOSITORY, copy_bounded,
                             extract_main, main_run_created, select_main_artifact)

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
                    'run_attempt': 1,
                    'status': 'completed', 'conclusion': 'success',
                    'event': 'push', 'path': '.github/workflows/web.yml',
                    'created_at': '2026-11-17T01:00:00Z',
                    'repository': {'full_name': REPOSITORY, 'id': 123},
                    'head_repository': {'full_name': REPOSITORY, 'id': 123}}
        self.artifact = {'id': 84, 'name': 'github-pages', 'expired': False,
                         'size_in_bytes': len(self.payload)}
        self.commit, self.live, self.active = COMMIT, b'stable', None

    def api(self, path):
        if path == 'git/ref/heads/current-work':
            return {'object': {'sha': self.commit}}
        if re.match(r'actions/runs/\d+/attempts/\d+/jobs\?', path):
            return {'jobs': publication_jobs(self.run)}
        if path.startswith('actions/runs/'):
            return {'artifacts': [self.artifact]}
        if path.startswith('actions/workflows/browser-publication-resume.yml/'):
            return {'workflow_runs': []}
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


def publication_jobs(run, conclusion='success'):
    return [{'name': name, 'run_id': run['id'], 'head_sha': run['head_sha'],
             'status': 'completed', 'conclusion': conclusion} for name in ('browser', 'deploy')]


class MainPublicationClient(FakeClient):
    def __init__(self):
        super().__init__()
        self.resume = self.run | {'id': 43, 'head_sha': 'c' * 40,
                                  'path': '.github/workflows/browser-publication-resume.yml',
                                  'event': 'workflow_dispatch', 'created_at': '2026-11-17T02:00:00Z'}
        self.runs = {'web.yml': [self.run], 'browser-publication-resume.yml': [self.resume]}
        self.artifacts = {42: [self.artifact], 43: [self.artifact | {'id': 85}]}
        self.jobs = {42: publication_jobs(self.run), 43: publication_jobs(self.resume)}
        self.resume_active = None

    def api(self, path):
        match = re.match(r'actions/runs/(\d+)/attempts/\d+/jobs\?', path)
        if match:
            return {'jobs': self.jobs.get(int(match[1]), [])}
        match = re.match(r'actions/workflows/([^/]+)/runs\?', path)
        if match and match[1] in self.runs:
            runs = self.runs[match[1]] if 'status=success' in path else []
            if (match[1] == 'browser-publication-resume.yml' and self.resume_active
                    and f'status={self.resume_active}' in path):
                runs = [self.resume]
            return {'workflow_runs': runs}
        match = re.match(r'actions/runs/(\d+)/artifacts', path)
        if match:
            return {'artifacts': self.artifacts.get(int(match[1]), [])}
        return super().api(path)


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

    def test_newest_trusted_publication_wins_across_both_workflows(self):
        client = MainPublicationClient()
        report = self.compose(client=client)
        self.assertEqual((report['preview']['main_run_id'], report['main_artifact_id']), (43, 85))
        self.assertTrue(verify_report(report, REPOSITORY, client)['verified'])
        client.run['created_at'] = client.resume['created_at']
        self.assertEqual(select_main_artifact(client)[0]['id'], 43)
        client.run['created_at'] = '2026-11-17T03:00:00Z'
        self.assertEqual(select_main_artifact(client)[0]['id'], 42)
        with self.assertRaisesRegex(ValueError, 'changed'):
            verify_report(report, REPOSITORY, client)
        client.runs['web.yml'].insert(0, client.run | {'id': 41, 'created_at': '2026-11-17T00:00:00Z'})
        self.assertEqual(select_main_artifact(client)[0]['id'], 42)

    def test_rejects_untrusted_resume_provenance_without_selecting_its_artifact(self):
        invalid = [{'event': event} for event in ('push', 'pull_request', 'workflow_run')]
        invalid += [{'conclusion': 'failure'}, {'status': 'in_progress'}, {'head_branch': 'current-work'},
                    {'path': '.github/workflows/web.yml'},
                    {'path': '.github/workflows/browser-publication-resume.yml@refs/pull/16/merge'},
                    {'repository': {'full_name': 'fork/repo', 'id': 123}},
                    {'head_repository': {'full_name': 'fork/repo', 'id': 123}},
                    {'head_repository': {'full_name': REPOSITORY, 'id': 456}},
                    {'repository': None}, {'head_repository': []}, {'id': True},
                    {'head_sha': 'invalid'}, {'created_at': 'invalid'}, {'created_at': None},
                    {'created_at': '2026-11-17T04:00:00'}]
        for update in invalid:
            client = MainPublicationClient()
            client.resume.update(update)
            with self.subTest(update=update):
                self.assertEqual(select_main_artifact(client)[0]['id'], 42)
                client.runs['web.yml'] = []
                with self.assertRaisesRegex(ValueError, 'trusted'):
                    select_main_artifact(client)
        self.assertIsNone(main_run_created(None, 'web.yml'))

    def test_accepts_only_explicit_main_workflow_refs_and_web_manual_runs(self):
        client = MainPublicationClient()
        for workflow, run in [('web.yml', client.run), ('browser-publication-resume.yml', client.resume)]:
            for suffix in ('', '@main', '@refs/heads/main'):
                with self.subTest(workflow=workflow, suffix=suffix):
                    self.assertIsNotNone(main_run_created(run | {
                        'path': f'.github/workflows/{workflow}{suffix}',
                        'event': 'workflow_dispatch'}, workflow))
            self.assertIsNone(main_run_created(run | {'event': 'pull_request'}, workflow))

    def test_scope_only_main_success_does_not_outrank_resume_publication(self):
        client = MainPublicationClient()
        client.run['created_at'] = '2026-11-17T03:00:00Z'
        client.jobs[42] = publication_jobs(client.run, 'skipped')
        client.artifacts[42] = []
        report = self.compose(client=client)
        self.assertEqual((report['preview']['main_run_id'], report['main_artifact_id']), (43, 85))
        self.assertTrue(verify_report(report, REPOSITORY, client)['verified'])
        client.runs['browser-publication-resume.yml'] = []
        with self.assertRaisesRegex(ValueError, 'trusted main publication'):
            select_main_artifact(client)

    def test_ambiguous_or_unsuccessful_jobs_cannot_be_ignored_as_scope_skips(self):
        for update in ('missing', 'duplicate', 'wrong_run', 'wrong_sha', 'incomplete',
                       'partial_skip', 'failure', 'missing_attempt'):
            client = MainPublicationClient()
            client.run['created_at'] = '2026-11-17T03:00:00Z'
            jobs = client.jobs[42]
            if update == 'missing':
                jobs.pop()
            elif update == 'duplicate':
                jobs.append(jobs[0].copy())
            elif update == 'wrong_run':
                jobs[0]['run_id'] = 999
            elif update == 'wrong_sha':
                jobs[0]['head_sha'] = 'd' * 40
            elif update == 'incomplete':
                jobs[0]['status'] = 'in_progress'
            elif update == 'partial_skip':
                jobs[1]['conclusion'] = 'skipped'
            elif update == 'failure':
                jobs[1]['conclusion'] = 'failure'
            else:
                client.run.pop('run_attempt')
            with self.subTest(update=update), self.assertRaises(ValueError):
                select_main_artifact(client, allow_expired=True)
        client = MainPublicationClient()
        client.jobs[43] = publication_jobs(client.resume, 'skipped')
        with self.assertRaises(ValueError):
            select_main_artifact(client)

    def test_successful_deploy_without_live_artifact_keeps_identity_instead_of_rollback(self):
        client = MainPublicationClient()
        client.run['created_at'] = '2026-11-17T03:00:00Z'
        client.artifacts[42] = []
        with self.assertRaisesRegex(ValueError, 'retained'):
            select_main_artifact(client)
        self.assertEqual(select_main_artifact(client, allow_expired=True), (client.run, None))
        client.artifacts[42] = [client.artifact | {'expired': True}]
        with self.assertRaisesRegex(ValueError, 'retained'):
            select_main_artifact(client)
        run, artifact = select_main_artifact(client, allow_expired=True)
        self.assertEqual((run['id'], artifact['id'], artifact['expired']), (42, 84, True))

    def test_active_or_expired_resume_cannot_roll_back_to_older_live_web(self):
        client = MainPublicationClient()
        for status in ('queued', 'in_progress', 'waiting', 'pending', 'requested'):
            client.resume_active = status
            with self.subTest(status=status), self.assertRaisesRegex(ValueError, 'active'):
                select_main_artifact(client)
        client.resume_active = None
        client.artifacts[43][0]['expired'] = True
        with self.assertRaisesRegex(ValueError, 'retained'):
            select_main_artifact(client)
        self.assertEqual(select_main_artifact(client, allow_expired=True)[0]['id'], 43)
        client.artifacts[43] = []
        self.assertEqual(select_main_artifact(client, allow_expired=True), (client.resume, None))

    def test_resume_expiry_renews_exact_retained_baseline_and_rejects_old_web_identity(self):
        from test_pages_baseline import BaselineClient, wrapped_baseline
        client = BaselineClient()
        resume = MainPublicationClient().resume
        original = client.api
        def api(path):
            if path.startswith('actions/runs/43/attempts/'):
                return {'jobs': publication_jobs(resume)}
            if path.startswith('actions/workflows/browser-publication-resume.yml/'):
                return {'workflow_runs': [resume] if 'status=success' in path else []}
            if path.startswith('actions/runs/43/artifacts'):
                return {'artifacts': [client.artifact | {'id': 85}]}
            return original(path)
        client.api = api
        record = {'schema': 1, 'main_run_id': 43, 'main_commit': resume['head_sha'],
                  'main_artifact_id': 85, 'zip_sha256': hashlib.sha256(client.payload).hexdigest()}
        client.wrapper = wrapped_baseline(client.payload, record)
        client.baseline_run.update(head_branch='main', event='workflow_run')
        baseline = self.root / 'baseline'
        report = self.compose(client=client, baseline_out=baseline)
        self.assertEqual(client.downloads, [200])
        self.assertEqual(json.loads((baseline / 'main.json').read_text()), record)
        self.assertTrue(verify_report(report, REPOSITORY, client)['verified'])
        client.wrapper = wrapped_baseline(client.payload)
        with self.assertRaisesRegex(ValueError, 'identity'):
            self.compose(client=client, out=self.root / 'reject-old')

    def test_actual_callback_gate_forces_trusted_resume_but_rejects_untrusted_events(self):
        workflow = (Path(__file__).resolve().parents[2] /
                    '.github/workflows/current-work-preview.yml').read_text(encoding='utf-8')
        expression = re.search(r'  prepare:\n    needs: scope\n    if: >-\n(.*?)    runs-on:',
                               workflow, re.S)[1]
        defaults = {'github.repository': REPOSITORY, 'github.event.repository.full_name': REPOSITORY,
                    'github.event_name': 'workflow_run', 'needs.scope.outputs.web': 'false',
                    'github.event.workflow_run.status': 'completed',
                    'github.event.workflow_run.conclusion': 'success',
                    'github.event.workflow_run.head_branch': 'main',
                    'github.event.workflow_run.head_repository.full_name': REPOSITORY,
                    'github.event.workflow_run.path': '.github/workflows/browser-publication-resume.yml',
                    'github.event.workflow_run.event': 'workflow_dispatch'}
        def allowed(**updates):
            values = defaults | updates
            translated = re.sub(r'\b(?:github|needs)\.[a-zA-Z_.]+',
                                lambda match: repr(values[match[0]]), expression)
            translated = ' '.join(translated.replace('&&', 'and').replace('||', 'or').split())
            return eval(translated, {'__builtins__': {}, 'fromJSON': json.loads,
                                    'contains': lambda collection, item: item in collection})
        self.assertTrue(allowed())
        for key, value in [('github.repository', 'fork/repo'),
                           ('github.event.repository.full_name', 'fork/repo'),
                           ('github.event.workflow_run.head_repository.full_name', 'fork/repo'),
                           ('github.event.workflow_run.conclusion', 'failure'),
                           ('github.event.workflow_run.status', 'in_progress'),
                           ('github.event.workflow_run.head_branch', 'current-work'),
                           ('github.event.workflow_run.path', '.github/workflows/untrusted.yml'),
                           ('github.event.workflow_run.event', 'push')]:
            with self.subTest(key=key, value=value):
                self.assertFalse(allowed(**{key: value}))
        self.assertFalse(allowed(**{'github.event_name': 'push'}))
        self.assertTrue(allowed(**{'github.event_name': 'push', 'needs.scope.outputs.web': 'true'}))
        self.assertFalse(allowed(**{'github.event.workflow_run.path': '.github/workflows/web.yml'}))
        self.assertTrue(allowed(**{'github.event.workflow_run.path': '.github/workflows/web.yml',
                                   'needs.scope.outputs.web': 'true'}))
        self.assertIn('workflows: [Browser preview, Resume qualified main browser publication]', workflow)
        self.assertIn("if: needs.prepare.result == 'success'", workflow)

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
