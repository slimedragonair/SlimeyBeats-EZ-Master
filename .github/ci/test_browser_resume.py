"""Offline evidence, real temporary Git history, and workflow-tail regressions."""
from copy import deepcopy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import HTTPError

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('browser_resume', HERE / 'browser_resume.py')
resume = importlib.util.module_from_spec(spec)
spec.loader.exec_module(resume)
RUN, JOB, ARTIFACT = 44029858651, 133858945527, 14306480858
HEAD = 'b' * 40
VERSION = '81b5082776573347'


def evidence():
    repository = dict(full_name=resume.REPOSITORY, id=1455935368)
    steps = [dict(name=name, number=i + 1, status='completed', conclusion=(
        'success' if i < len(resume.PRIOR) else 'failure' if i == len(resume.PRIOR) else 'skipped'))
        for i, name in enumerate(resume.PRIOR + resume.REMAINING)]
    steps += [dict(name='Run actions/upload-artifact@v7', number=23,
                   status='completed', conclusion='success')]
    return {
        f'actions/runs/{RUN}': dict(id=RUN, workflow_id=416804616, path=resume.WORKFLOW,
            event='push', head_branch='main', head_sha=resume.BASELINE_HEAD, status='completed',
            conclusion='failure', repository=repository, head_repository=repository.copy(), run_attempt=1),
        'actions/workflows/web.yml': dict(id=416804616, path=resume.WORKFLOW),
        f'actions/runs/{RUN}/attempts/1/jobs?per_page=100': dict(total_count=1, jobs=[dict(
            id=JOB, name='browser', run_id=RUN, head_sha=resume.BASELINE_HEAD,
            status='completed', conclusion='failure', steps=steps)]),
        f'actions/runs/{RUN}/artifacts?per_page=100': dict(total_count=1, artifacts=[dict(
            id=ARTIFACT, name=resume.ARTIFACT, expired=False, size_in_bytes=375595382,
            digest='sha256:6677e586014d25c60ee7f35d8f666fc45cd514ea1134c9daff202c542b62f099',
            workflow_run=dict(id=RUN, head_sha=resume.BASELINE_HEAD, head_branch='main',
                              repository_id=1455935368, head_repository_id=1455935368))]),
    }


class ApiTests(unittest.TestCase):
    def test_metadata_needs_token_and_http_errors_do_not_echo_credentials(self):
        with patch.dict(os.environ, {}, clear=True):
            with self.assertRaisesRegex(ValueError, 'GITHUB_TOKEN'):
                resume.api('actions/runs/1')
        with patch.dict(os.environ, GITHUB_TOKEN='not-a-real-token'):
            error = HTTPError('https://api.github.com/', 403, 'not-a-real-token', {}, None)
            with patch.object(resume, 'urlopen', side_effect=error):
                with self.assertRaisesRegex(ValueError, 'HTTP 403') as caught:
                    resume.api('actions/runs/1')
            self.assertNotIn('not-a-real-token', str(caught.exception))


class BaselineTests(unittest.TestCase):
    def verify(self, data):
        with patch.object(resume, 'source_guard', return_value=['web/tools/region_edit_ui_checks.py']):
            return resume.baseline(RUN, Path('.'), HEAD, data.__getitem__)

    def test_accepts_exact_failure_and_records_reused_and_required_steps(self):
        receipt = self.verify(evidence())
        self.assertEqual(receipt['baseline_job'], JOB)
        self.assertEqual(receipt['artifact_id'], ARTIFACT)
        self.assertEqual(receipt['reused_successful_steps'], resume.PRIOR)
        self.assertEqual(receipt['rerun_required_steps'], resume.REMAINING)
        self.assertEqual(receipt['baseline_steps'][17]['conclusion'], 'failure')

    def test_rejects_foreign_runs_wrong_workflow_event_branch_head_and_attempt(self):
        for field, value in [('id', RUN + 1), ('path', '.github/workflows/other.yml'),
                             ('event', 'pull_request'), ('head_branch', 'current-work'),
                             ('head_sha', HEAD), ('conclusion', 'success'), ('status', 'in_progress'),
                             ('workflow_id', 42), ('run_attempt', 2)]:
            with self.subTest(field=field):
                data = evidence()
                data[f'actions/runs/{RUN}'][field] = value
                with self.assertRaises((ValueError, KeyError)):
                    self.verify(data)
        for key in ('repository', 'head_repository'):
            data = evidence()
            data[f'actions/runs/{RUN}'][key] = dict(full_name='foreign/repo', id=42)
            with self.assertRaises(ValueError):
                self.verify(data)

    def test_every_prior_step_is_required_and_cannot_be_skipped(self):
        for index in range(len(resume.PRIOR)):
            for conclusion in ('skipped', 'failure', 'cancelled', None):
                with self.subTest(index=index, conclusion=conclusion):
                    data = evidence()
                    data[f'actions/runs/{RUN}/attempts/1/jobs?per_page=100']['jobs'][0]['steps'][index]['conclusion'] = conclusion
                    with self.assertRaises(ValueError):
                        self.verify(data)

    def test_rejects_missing_steps_multiple_failures_unexpected_tail_and_incomplete_jobs(self):
        for case in ('missing', 'order', 'timeline', 'tail', 'extra_failure', 'jobs', 'duplicate', 'job_head'):
            with self.subTest(case=case):
                data = evidence()
                jobs = data[f'actions/runs/{RUN}/attempts/1/jobs?per_page=100']
                steps = jobs['jobs'][0]['steps']
                if case == 'missing':
                    del steps[5]
                elif case == 'order':
                    steps[4]['name'], steps[5]['name'] = steps[5]['name'], steps[4]['name']
                elif case == 'timeline':
                    steps[17]['conclusion'] = 'success'
                elif case == 'tail':
                    steps[18]['conclusion'] = 'success'
                elif case == 'extra_failure':
                    steps[-1]['conclusion'] = 'failure'
                elif case == 'jobs':
                    jobs['total_count'] = 101
                elif case == 'duplicate':
                    jobs['jobs'].append(deepcopy(jobs['jobs'][0]))
                    jobs['total_count'] = 2
                else:
                    jobs['jobs'][0]['head_sha'] = HEAD
                with self.assertRaises(ValueError):
                    self.verify(data)

    def test_rejects_expired_missing_duplicate_unbounded_or_foreign_artifact(self):
        for case in ('missing', 'duplicate', 'expired', 'size', 'digest', 'origin', 'partial'):
            with self.subTest(case=case):
                data = evidence()
                listing = data[f'actions/runs/{RUN}/artifacts?per_page=100']
                item = listing['artifacts'][0]
                if case == 'missing':
                    listing.update(total_count=0, artifacts=[])
                elif case == 'duplicate':
                    listing['artifacts'].append(item.copy())
                    listing['total_count'] = 2
                elif case == 'expired':
                    item['expired'] = True
                elif case == 'size':
                    item['size_in_bytes'] = 513 * 1024 * 1024
                elif case == 'digest':
                    del item['digest']
                elif case == 'origin':
                    item['workflow_run']['repository_id'] = 42
                else:
                    listing['total_count'] = 101
                with self.assertRaises(ValueError):
                    self.verify(data)


class GitTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.git('init', '-q')
        self.git('config', 'user.email', 'offline@example.invalid')
        self.git('config', 'user.name', 'Offline test')
        self.write('web/site/app.js', 'const baseline = true;')
        self.git('add', '.')
        self.git('commit', '-qm', 'baseline')
        self.base = self.git('rev-parse', 'HEAD')
        self.addCleanup(patch.stopall)
        patch.object(resume, 'BASELINE_HEAD', self.base).start()

    def git(self, *args):
        return resume.git(self.root, *args)

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value, encoding='utf-8')

    def commit(self):
        self.git('add', '.')
        self.git('commit', '-qm', 'next')
        return self.git('rev-parse', 'HEAD')

    def guard(self, head, remote=None):
        return resume.source_guard(self.root, head, lambda path: dict(object=dict(sha=remote or head)))

    def test_exact_validator_regression_policy_and_documentation_are_allowed(self):
        paths = ['web/tools/region_edit_ui_checks.py', 'web/tools/test_region_edit_download.py',
                 '.github/ci/browser_resume.py', '.github/ci/check_scope.py', 'docs/CI.md', 'README.md',
                 'web/tools/pages_artifacts.py', 'web/tools/pages_baseline.py',
                 'web/tools/compose_pages_preview.py', 'web/tools/test_pages_preview.py',
                 'web/tools/test_pages_baseline.py', 'web/tools/test_pages_artifacts.py',
                 '.github/workflows/current-work-preview.yml',
                 'mcp/tools/verify_optional_tools.py', 'mcp/tools/verify_connect_config.ps1',
                 '.github/workflows/ci.yml']
        for name in paths:
            self.write(name, 'test-only')
        self.assertEqual(set(self.guard(self.commit())), set(paths))

    def test_application_build_encoder_assets_and_unknown_test_changes_reject(self):
        paths = ['web/site/app.js', 'web/dsp/src/lib.rs', 'rust/src/lib.rs', 'rust/assets/a.json',
                 'web/dsp/Cargo.lock', 'Cargo.toml', 'examples/audio/CREDITS.md', 'LICENSE',
                 'web/tools/build.py', 'web/tools/runtime_cache.py', 'web/tools/encoder.py',
                 '.github/workflows/web.yml', 'web/tools/verify_studio_live.py', 'unknown.md',
                 'web/tools/unknown_publication_policy.py', 'web/site/README.md',
                 'web/tools/test_region_edit_ui_checks.py',
                 'mcp/tools/Connect-AI-Assistant.ps1', 'mcp/server/src/main.rs',
                 'mcp/tools/verify_unknown.py']
        for name in paths:
            with self.subTest(path=name):
                self.git('reset', '--hard', self.base)
                self.write(name, 'changed')
                with self.assertRaisesRegex(ValueError, 'full web.yml validation'):
                    self.guard(self.commit())

    def test_rejects_moved_main_checkout_mismatch_dirty_and_untracked_files(self):
        self.write('README.md', 'doc')
        head = self.commit()
        with self.assertRaisesRegex(ValueError, 'Main advanced'):
            self.guard(head, 'a' * 40)
        with self.assertRaisesRegex(ValueError, 'Checkout differs'):
            self.guard(self.base)
        self.write('web/site/app.js', 'local change')
        with self.assertRaisesRegex(ValueError, 'local changes'):
            self.guard(head)
        self.git('reset', '--hard', head)
        self.write('web/site/untracked.js', 'new code')
        with self.assertRaisesRegex(ValueError, 'local changes'):
            self.guard(head)

    def test_baseline_must_be_an_ancestor(self):
        self.git('checkout', '--orphan', 'foreign')
        self.git('rm', '-rf', '.')
        self.write('README.md', 'unrelated')
        with self.assertRaises(subprocess.CalledProcessError):
            self.guard(self.commit())


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        for browser in ('chromium', 'firefox', 'webkit'):
            self.report(browser, VERSION)

    def report(self, browser, version, success=True):
        path = self.root / f'bootstrap-{browser}' / 'bootstrap-verification.json'
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(dict(browser=browser + ' 123.4', runtime_version=version,
                                        success=success)), encoding='utf-8')
        return path

    def test_reads_only_three_passed_json_reports_and_records_hashes(self):
        # A large fixture and unrelated JSON are never opened or scanned.
        (self.root / 'fixture.wav').write_bytes(b'not JSON')
        (self.root / 'failure.json').write_text('invalid JSON', encoding='utf-8')
        version, hashes = resume.report_runtime(self.root)
        self.assertEqual(version, VERSION)
        self.assertEqual(len(hashes), 3)
        self.assertTrue(all(len(value) == 64 for value in hashes.values()))

    def test_rejects_disagreement_failure_malformed_missing_and_oversized_reports(self):
        for case in ('disagree', 'failure', 'malformed', 'missing', 'oversized', 'wrong_browser'):
            with self.subTest(case=case):
                path = self.report('webkit', VERSION)
                if case == 'disagree':
                    self.report('webkit', 'f' * 16)
                elif case == 'failure':
                    self.report('webkit', VERSION, False)
                elif case == 'malformed':
                    path.write_text('bad JSON', encoding='utf-8')
                elif case == 'missing':
                    path.unlink()
                elif case == 'oversized':
                    path.write_bytes(b' ' * (resume.JSON_LIMIT + 1))
                else:
                    path.write_text(json.dumps(dict(browser='chromium 123', success=True,
                                                   runtime_version=VERSION)), encoding='utf-8')
                with self.assertRaises((ValueError, OSError)):
                    resume.report_runtime(self.root)

    def test_full_site_hash_catches_mutation_and_runtime_must_be_uniform(self):
        (self.root / 'index.html').write_text(f'<script src="app.js?v={VERSION}"></script>', encoding='utf-8')
        (self.root / 'app.js').write_bytes(b'original')
        first = resume.site_identity(self.root)
        self.assertEqual(first[0], VERSION)
        (self.root / 'app.js').write_bytes(b'changed')
        self.assertNotEqual(resume.site_identity(self.root)[1], first[1])
        (self.root / 'index.html').write_text('<script src="app.js"></script>', encoding='utf-8')
        with self.assertRaises(ValueError):
            resume.site_identity(self.root)
        (self.root / 'index.html').write_text(
            f'<script src="app.js?v={VERSION}"></script><link href="a.css?v={"f" * 16}">', encoding='utf-8')
        with self.assertRaises(ValueError):
            resume.site_identity(self.root)

    def test_qualify_and_publish_guard_preserve_receipt_on_mismatch(self):
        site = self.root / 'site'
        site.mkdir()
        index = site / 'index.html'
        index.write_text(f'<script src="app.js?v={VERSION}"></script>', encoding='utf-8')
        receipt_path = self.root / 'receipt.json'
        initial = dict(repository=resume.REPOSITORY, head=HEAD, baseline_head=resume.BASELINE_HEAD,
                       changed_paths=['web/tools/region_edit_ui_checks.py'])
        receipt_path.write_text(json.dumps(initial), encoding='utf-8')
        env = dict(GITHUB_REPOSITORY=resume.REPOSITORY, GITHUB_REF='refs/heads/main',
                   GITHUB_EVENT_NAME='workflow_dispatch', GITHUB_SHA=HEAD)
        args = ['--receipt', str(receipt_path), '--site', str(site)]
        with patch.dict(os.environ, env), patch.object(resume, 'source_guard', return_value=initial['changed_paths']):
            resume.main(['qualify', *args, '--reports', str(self.root)])
            qualified = receipt_path.read_bytes()
            self.assertEqual(json.loads(qualified)['runtime_version'], VERSION)
            resume.main(['guard', *args])
            self.assertEqual(receipt_path.read_bytes(), qualified)
            (site / 'injected.js').write_bytes(b'not qualified')
            with self.assertRaisesRegex(ValueError, 'site bytes changed'):
                resume.main(['guard', *args])
            self.assertEqual(receipt_path.read_bytes(), qualified)
            index.write_text('<script src="app.js?v=ffffffffffffffff"></script>', encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'differs from passed baseline'):
                resume.main(['qualify', *args, '--reports', str(self.root)])
            self.assertEqual(receipt_path.read_bytes(), qualified)


def workflow_steps(text):
    import re
    return dict(re.findall(r'^      - name: ([^\n]+)\n(.*?)(?=^      - |^  [a-z]|\Z)', text, re.M | re.S))


class WorkflowTests(unittest.TestCase):
    def test_failed_step_and_every_remaining_web_validation_command_are_identical(self):
        workflows = HERE.parent / 'workflows'
        baseline = workflow_steps((workflows / 'web.yml').read_text(encoding='utf-8'))
        recovery = workflow_steps((workflows / 'browser-publication-resume.yml').read_text(encoding='utf-8'))
        for name in resume.REMAINING:
            with self.subTest(step=name):
                self.assertEqual(recovery[name], baseline[name])
        tail = list(baseline)[list(baseline).index(resume.REMAINING[0]):]
        self.assertEqual(tail[:-1], resume.REMAINING)

    def test_manual_only_read_only_metadata_cross_run_download_and_guarded_pages(self):
        text = (HERE.parent / 'workflows/browser-publication-resume.yml').read_text(encoding='utf-8')
        self.assertIn('  workflow_dispatch:', text)
        self.assertNotIn('  push:', text)
        self.assertNotIn('pull_request', text)
        self.assertIn('actions: read', text)
        self.assertIn('uses: actions/download-artifact@v8', text)
        self.assertIn('run-id: ${{ inputs.baseline_run }}', text)
        self.assertIn('github-token: ${{ github.token }}', text)
        self.assertIn('group: pages-publication', text)
        self.assertLess(text.rfind('browser_resume.py guard'), text.index('uses: actions/deploy-pages@v4'))
        self.assertIn('reused_successful_steps', (HERE / 'browser_resume.py').read_text(encoding='utf-8'))

    def test_dispatch_gate_rejects_other_refs_and_events_before_api_calls(self):
        for ref, event in [('refs/heads/current-work', 'workflow_dispatch'), ('refs/heads/main', 'push')]:
            with patch.dict(os.environ, GITHUB_REPOSITORY=resume.REPOSITORY,
                            GITHUB_REF=ref, GITHUB_EVENT_NAME=event):
                with self.assertRaisesRegex(ValueError, 'Manual main dispatch only'):
                    resume.main(['baseline', '--receipt', 'unused.json'])


if __name__ == '__main__':
    unittest.main()
