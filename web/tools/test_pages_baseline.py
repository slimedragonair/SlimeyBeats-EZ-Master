"""Offline expiry, renewal, identity and malicious baseline regression tests."""
import hashlib
import io
import json
from pathlib import Path
import stat
import tempfile
import unittest
from unittest.mock import Mock, patch
import warnings
import zipfile

from compose_pages_preview import compose, verify_report
from pages_artifacts import REPOSITORY
from pages_baseline import read_baseline, retrieve_main, write_baseline
from test_pages_preview import COMMIT, FakeClient, PREVIEW, RUN_URL


def wrapped_baseline(payload, record=None, entries=None):
    record = record or {'schema': 1, 'main_run_id': 42, 'main_commit': 'b' * 40,
                        'main_artifact_id': 84, 'zip_sha256': hashlib.sha256(payload).hexdigest()}
    result = io.BytesIO()
    with zipfile.ZipFile(result, 'w', zipfile.ZIP_DEFLATED) as zipped:
        for name, data in entries or [('main.zip', payload), ('main.json', json.dumps(record).encode())]:
            zipped.writestr(name, data)
    return result.getvalue()


class BaselineClient(FakeClient):
    def __init__(self):
        super().__init__()
        self.artifact['expired'] = True
        self.wrapper = wrapped_baseline(self.payload)
        self.baseline_run = self.run | {'id': 100, 'head_branch': 'current-work', 'event': 'push'}
        self.baseline_artifact = {'id': 200, 'name': 'published-main-baseline',
                                  'expired': False, 'size_in_bytes': len(self.wrapper)}
        self.downloads = []

    def api(self, path):
        if path.startswith('actions/workflows/current-work-preview.yml/runs?'):
            return {'workflow_runs': [self.baseline_run]}
        if path.startswith('actions/runs/100/artifacts'):
            return {'artifacts': [self.baseline_artifact]}
        return super().api(path)

    def download(self, artifact_id, destination):
        self.downloads.append(artifact_id)
        destination.write(self.wrapper if artifact_id == 200 else self.payload)


class BaselineTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.preview = self.root / 'preview'
        self.preview.mkdir()
        (self.preview / 'index.html').write_bytes(PREVIEW)
        self.client = BaselineClient()

    def compose(self, **overrides):
        arguments = dict(preview_site=self.preview, out=self.root / 'site', repository=REPOSITORY,
                         commit=COMMIT, run_url=RUN_URL, client=self.client,
                         baseline_out=self.root / 'baseline')
        return compose(**(arguments | overrides))

    def test_fresh_main_retains_exact_zip_and_schema_one_provenance(self):
        self.client.artifact['expired'] = False
        report = self.compose()
        baseline = self.root / 'baseline'
        self.assertEqual({path.name for path in baseline.iterdir()}, {'main.zip', 'main.json'})
        self.assertEqual((baseline / 'main.zip').read_bytes(), self.client.payload)
        record = json.loads((baseline / 'main.json').read_text())
        self.assertEqual(record, {'schema': 1, 'main_run_id': 42, 'main_commit': 'b' * 40,
                                 'main_artifact_id': 84,
                                 'zip_sha256': hashlib.sha256(self.client.payload).hexdigest()})
        self.assertEqual(self.client.downloads, [84])
        self.assertEqual(report['main_artifact_id'], 84)
        self.assertFalse((self.root / 'site/main.zip').exists())

    def test_expired_main_uses_callback_baseline_and_renews_bytes_with_guard_still_valid(self):
        self.client.baseline_run.update(head_branch='main', event='workflow_run')
        report = self.compose()
        self.assertEqual(self.client.downloads, [200])
        self.assertEqual((self.root / 'baseline/main.zip').read_bytes(), self.client.payload)
        self.assertEqual((self.root / 'site/index.html').read_bytes(), b'stable')
        self.assertTrue(verify_report(report, REPOSITORY, self.client)['verified'])
        self.client.wrapper = wrapped_baseline(
            (self.root / 'baseline/main.zip').read_bytes(),
            json.loads((self.root / 'baseline/main.json').read_text()))
        renewed = self.compose(out=self.root / 'site-two', baseline_out=self.root / 'baseline-two')
        self.assertEqual(renewed['main_files'], report['main_files'])
        self.assertEqual((self.root / 'baseline-two/main.zip').read_bytes(), self.client.payload)

    def test_missing_original_artifact_can_use_proven_baseline(self):
        original = self.client.api
        self.client.api = lambda path: {'artifacts': []} if path.startswith('actions/runs/42/') else original(path)
        report = self.compose()
        self.assertEqual(report['main_artifact_id'], 84)
        self.assertTrue(verify_report(report, REPOSITORY, self.client)['verified'])

    def test_accepts_owned_current_work_and_main_preview_events(self):
        for branch, event in [('current-work', 'push'), ('current-work', 'workflow_dispatch'),
                              ('main', 'workflow_run'), ('main', 'workflow_dispatch')]:
            client = BaselineClient()
            client.baseline_run.update(head_branch=branch, event=event)
            client.api = Mock(wraps=client.api)
            with self.subTest(branch=branch, event=event):
                destination = io.BytesIO()
                run, artifact = retrieve_main(client, destination)
                self.assertEqual(destination.read(), client.payload)
                self.assertEqual((run['id'], artifact['id']), (42, 84))
                queries = [call.args[0] for call in client.api.call_args_list
                           if 'actions/workflows/current-work-preview.yml/runs?' in call.args[0]]
                self.assertTrue(queries)
                self.assertTrue(all('branch=' not in query for query in queries))

    def test_changed_main_or_public_index_cannot_fall_back_to_older_main(self):
        for field, value in [('id', 43), ('head_sha', 'c' * 40)]:
            client = BaselineClient()
            client.baseline_run.update(head_branch='main', event='workflow_run')
            client.run[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.compose(client=client)
            self.assertFalse((self.root / 'site').exists())
        self.client.live = b'changed production'
        with self.assertRaises(ValueError):
            self.compose()
        self.assertFalse((self.root / 'baseline').exists())

    def test_rejects_hash_schema_and_original_artifact_identity_tampering(self):
        record = json.loads(zipfile.ZipFile(io.BytesIO(self.client.wrapper)).read('main.json'))
        for field, value in [('schema', 2), ('schema', True), ('main_run_id', True),
                             ('main_artifact_id', 85), ('zip_sha256', '0' * 64)]:
            self.client.wrapper = wrapped_baseline(self.client.payload, record | {field: value})
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                retrieve_main(self.client, io.BytesIO())

    def test_rejects_untrusted_failed_other_branch_or_expired_baseline_runs(self):
        for update in [{'head_branch': 'main'}, {'head_branch': 'unsafe', 'event': 'workflow_dispatch'},
                       {'event': 'workflow_run'}, {'event': 'pull_request'}, {'event': None},
                       {'conclusion': 'failure'}, {'status': 'in_progress'},
                       {'head_repository': {'full_name': 'other/repo'}},
                       {'head_branch': 'main', 'event': 'workflow_run',
                        'head_repository': {'full_name': 'fork/repo'}}]:
            client = BaselineClient()
            client.baseline_run.update(update)
            with self.subTest(update=update), self.assertRaises(ValueError):
                retrieve_main(client, io.BytesIO())
        self.client.baseline_artifact['expired'] = True
        with self.assertRaises(ValueError):
            retrieve_main(self.client, io.BytesIO())

    def test_rejects_extra_traversing_duplicate_link_and_oversized_wrapper_entries(self):
        link = zipfile.ZipInfo('main.zip')
        link.create_system, link.external_attr = 3, (stat.S_IFLNK | 0o777) << 16
        for entries in [[('main.zip', b'x')], [('main.zip', b'x'), ('../main.json', b'{}')],
                        [('main.zip', b'x'), ('main.json', b'{}'), ('extra', b'x')],
                        [(link, b'target'), ('main.json', b'{}')],
                        [('main.zip', b'x'), ('main.json', b' ' * 16385)]]:
            with self.subTest(entries=str(entries)[:80]), self.assertRaises(ValueError):
                read_baseline(io.BytesIO(wrapped_baseline(b'', entries=entries)), io.BytesIO(),
                              self.client.run, self.client.artifact)
        with patch('pages_baseline.MAX_BYTES', 1), self.assertRaises(ValueError):
            read_baseline(io.BytesIO(self.client.wrapper), io.BytesIO(), self.client.run, self.client.artifact)
        with warnings.catch_warnings():
            warnings.simplefilter('ignore', UserWarning)
            duplicate = wrapped_baseline(b'', entries=[('main.zip', b'x'), ('main.zip', b'y')])
        with self.assertRaises(ValueError):
            read_baseline(io.BytesIO(duplicate), io.BytesIO(), self.client.run, self.client.artifact)

    def test_baseline_output_must_be_fresh_and_outside_both_site_trees(self):
        for destination in [self.preview, self.preview / 'baseline', self.root,
                            self.root / 'site/baseline', self.root / 'site']:
            with self.subTest(destination=destination), self.assertRaises(ValueError):
                self.compose(baseline_out=destination)
        existing = self.root / 'existing'
        existing.mkdir()
        with self.assertRaises(ValueError):
            write_baseline(io.BytesIO(self.client.payload), existing,
                           self.client.run, self.client.artifact)


if __name__ == '__main__':
    unittest.main()
