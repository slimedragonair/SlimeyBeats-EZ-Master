"""Usable configurable guest budgets keep independent finite admission/rate caps."""
from dataclasses import replace
import os
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from hybrid.server.arrangement_archives import ArchiveReview
from hybrid.server.arrangement_session import RemoteArrangement
from hybrid.server.config import from_arguments
from hybrid.server.errors import ApiError
from hybrid.server.guest_sessions import GuestRegistry
from hybrid.tests.project_support import project_service, create_project, upload
from hybrid.tests.support import mint_guest, TOKEN


class GuestPolicyTests(unittest.TestCase):
    def registry(self, session=16, ip=32, total=128):
        config = SimpleNamespace(public_enabled=True, guest_session_ttl_seconds=3600,
                                 guest_jobs_per_session=session, guest_jobs_per_ip_hour=ip, guest_jobs_per_hour=total)
        return GuestRegistry(config, monotonic=lambda: 0, wall_time=lambda: 1700000000)

    def test_public_defaults_cli_configuration_and_finite_validation(self):
        with project_service() as (service, client):
            status, _, caps = client.request('GET', '/v2/capabilities')
            self.assertEqual(status, 200)
            self.assertEqual([caps['limits'][key] for key in ('guest_jobs_per_session', 'guest_jobs_per_ip_hour', 'guest_jobs_per_hour')], [16, 32, 128])
            self.assertEqual(caps['limits']['guest_admission_commands_per_session'], 120)
            self.assertFalse(caps['arrangement']['archive_admission_uses_heavy_quota'])
            for field, maximum in [('guest_jobs_per_session', 256), ('guest_jobs_per_ip_hour', 1024), ('guest_jobs_per_hour', 4096)]:
                for value in (0, True, 1.5, maximum + 1):
                    with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                        replace(service.config, **{field: value})
        with patch.dict(os.environ, {'SLIMEYBEATS_HYBRID_TOKEN': TOKEN}):
            configured = from_arguments(['--cli', sys.executable, '--guest-jobs-per-session', '4',
                                         '--guest-jobs-per-ip-hour', '8', '--guest-jobs-per-hour', '32'])
        self.assertEqual((configured.guest_jobs_per_session, configured.guest_jobs_per_ip_hour, configured.guest_jobs_per_hour), (4, 8, 32))

    def test_coded_session_ip_and_global_limits_and_legacy_wording(self):
        for registry, peer_rotation, expected in [(self.registry(session=1), False, 'guest_job_limit'),
                                                 (self.registry(ip=1), False, 'guest_job_ip_limit'),
                                                 (self.registry(total=1), True, 'public_job_limit')]:
            first_key = registry.client_key('198.51.100.1')
            first = registry.authenticate(registry.mint(first_key)['token'])
            registry.start_job(first, first_key, coded=True)
            next_key = registry.client_key('198.51.100.2') if peer_rotation else first_key
            principal = first if expected == 'guest_job_limit' else registry.authenticate(registry.mint(next_key)['token'])
            with self.subTest(expected=expected), self.assertRaises(ApiError) as caught:
                registry.start_job(principal, next_key, coded=True)
            self.assertEqual((caught.exception.status, caught.exception.message), (429, expected))
            self.assertGreater(caught.exception.retry_after, 0)
        registry = self.registry(session=1)
        key = registry.client_key('198.51.100.1')
        principal = registry.authenticate(registry.mint(key)['token'])
        registry.start_job(principal, key)
        with self.assertRaises(ApiError) as caught:
            registry.start_job(principal, key)
        self.assertEqual(caught.exception.message, 'Guest job rate limit reached')
        for _ in range(120):
            registry.start_command(principal, key, coded=True)
        with self.assertRaises(ApiError) as caught:
            registry.start_command(principal, key, coded=True)
        self.assertEqual(caught.exception.message, 'guest_admission_limit')

    def test_http_archive_admission_is_independent_and_preserves_specific_429(self):
        def complete(store, state, job):
            with store.jobs.changed:
                job.state, job.result = 'completed', {}
                state.active_job = None
                store.jobs.arrangement_active = None
                job.done.set()
                store.jobs.changed.notify_all()
        with project_service(public_enabled=True, v2_arrangement_enabled=True, guest_jobs_per_session=1) as (service, owner), \
                patch('hybrid.server.arrangement_jobs.execute', complete):
            client, _ = mint_guest(owner)
            project = create_project(client)
            private = service.store.projects.projects[project['id']]
            state = private.arrangement = RemoteArrangement(service.config, private)
            state.snapshot = {'epoch': 'a' * 32, 'revision': 0, 'arrangement': {'eq': []}}
            uploaded = upload(client, project['id'], b'archive', role='arrangement_archive', filename='Parts.zip')
            path = f'/v2/projects/{project["id"]}/arrangement/jobs'
            def submit(operation, **fields):
                return client.request('POST', path, {'operation': operation, 'epoch': 'a' * 32, 'expected_revision': 0, **fields})
            self.assertEqual(submit('review_archive', upload_id=uploaded)[0], 202)
            state.reviews['1' * 32] = ArchiveReview('1' * 32, uploaded,
                {'2' * 32: {'name': 'Voice', 'versions': {'3' * 32: {'index': 0, 'bytes': 7}}}}, 1)
            self.assertEqual(submit('commit_import', review_id='1' * 32,
                selection=[{'part_id': '2' * 32, 'version_id': '3' * 32}])[0], 202)
            principal = service.store.sessions.authenticate(client.token)
            quota = service.store.sessions._session(principal, service.store.sessions.monotonic())
            self.assertEqual((quota.job_starts, quota.command_starts), (0, 2))
            self.assertEqual(submit('save_project')[0], 202)
            status, headers, response = submit('save_project')
            self.assertEqual((status, response), (429, {'error': 'guest_job_limit'}))
            self.assertGreater(int(headers['Retry-After']), 0)
            self.assertEqual((quota.job_starts, quota.command_starts), (1, 2))


if __name__ == '__main__':
    unittest.main()
