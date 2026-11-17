"""HTTP admission stays usable without discarding live work or owned artifacts."""
from contextlib import contextmanager
import secrets
import threading
import time
import unittest
from unittest.mock import patch

from hybrid.server.arrangement_job_artifacts import register
from hybrid.server.arrangement_job_state import ArrangementJob, MAX_JOB_RECORDS, MAX_EXPIRED_JOB_IDS
from hybrid.server.arrangement_session import RemoteArrangement
from hybrid.tests.project_support import project_service, create_project
from hybrid.tests.support import mint_guest


class ArrangementJobHistoryTests(unittest.TestCase):
    @contextmanager
    def workspace(self):
        with project_service(v2_arrangement_enabled=True, public_enabled=True) as (service, client):
            manifest = create_project(client)
            store = service.store.projects
            project = store.projects[manifest['id']]
            state = project.arrangement = RemoteArrangement(store.config, project)
            state.snapshot = {'epoch': 'a' * 32, 'revision': 1, 'arrangement': {'eq': []}}
            try:
                yield service, client, project, state, f'/v2/projects/{project.id}/arrangement/jobs'
            finally:
                # Only this fixture's synthetic records have no native actor.
                for job in state.jobs.values():
                    if job.native is None:
                        job.unreaped = False

    def request(self):
        return {'operation': 'suggest_eq', 'epoch': 'a' * 32, 'expected_revision': 1,
                'target': {'scope': {'kind': 'mix'}}}

    @staticmethod
    def complete(store, state, job):
        # Replace DSP only; exercise real HTTP ownership, threaded admission,
        # polling and retention with an inexpensive successful proposal.
        with store.jobs.changed:
            job.state, job.result = 'completed', {'bands': [], 'notes': []}
            state.active_job = None
            store.jobs.arrangement_active = None
            job.done.set()
            store.jobs.changed.notify_all()

    def submit_completed(self, client, path):
        status, _, job = client.request('POST', path, self.request())
        self.assertEqual(status, 202, job)
        deadline = time.monotonic() + 5
        while job['state'] in ('queued', 'running'):
            self.assertLess(time.monotonic(), deadline)
            status, _, job = client.request('GET', path + '/' + job['job_id'])
            self.assertEqual(status, 200, job)
        self.assertEqual(job['state'], 'completed', job)
        return job['job_id']

    def test_successive_jobs_expire_records_and_preserve_download_ownership(self):
        with self.workspace() as (service, client, project, state, path), \
                patch('hybrid.server.arrangement_jobs.execute', self.complete):
            first = self.submit_completed(client, path)
            job = state.jobs[first]
            job.stage.mkdir()
            asset = job.stage / 'saved.wav'
            asset.write_bytes(b'owned downloadable artifact')
            artifact = register(service.store.projects, project, job, asset, 'Studio.wav', 'audio/wav')
            for _ in range(MAX_JOB_RECORDS + MAX_EXPIRED_JOB_IDS + 3):
                last = self.submit_completed(client, path)
            self.assertEqual(len(state.jobs), MAX_JOB_RECORDS)
            self.assertEqual(len(state.expired_jobs), MAX_EXPIRED_JOB_IDS)
            expired = next(iter(state.expired_jobs))
            for method, suffix in [('GET', ''), ('POST', '/cancel')]:
                status, _, response = client.request(method, path + '/' + expired + suffix,
                                                     {} if method == 'POST' else None)
                self.assertEqual((status, response['error']), (410, 'arrangement_job_expired'))
            self.assertEqual(client.request('GET', path + '/' + first)[0], 404)
            self.assertEqual(client.request('GET', path + '/' + last)[0], 200)
            self.assertEqual(client.request('GET', path + '/' + secrets.token_hex(16))[0], 404)
            download = f'/v2/projects/{project.id}/artifacts/{artifact["id"]}'
            self.assertTrue(job.keep_stage)
            self.assertEqual(client.request('GET', download)[2], b'owned downloadable artifact')
            guest, _ = mint_guest(client)
            self.assertEqual(guest.request('GET', download)[0], 404)
            self.assertEqual(guest.request('GET', path + '/' + expired)[0], 404)
            self.assertEqual(guest.request('POST', path + '/' + expired + '/cancel', {})[0], 404)

    def test_active_poll_cancel_and_capacity_keep_unreaped_or_unfinished_records(self):
        with self.workspace() as (_, client, project, state, path):
            for index in range(MAX_JOB_RECORDS):
                job_id = f'{index:032x}'
                job = ArrangementJob(job_id, 'suggest_eq', 'a' * 32, 1, project.directory / job_id, {}, {})
                job.state = 'completed'
                job.done.set()
                job.unreaped = True
                state.jobs[job_id] = job
            status, _, response = client.request('POST', path, self.request())
            self.assertEqual((status, response['error']), (429, 'arrangement_job_history_limit'))
            oldest = next(iter(state.jobs.values()))
            oldest.unreaped = False
            oldest.done.clear()
            self.assertEqual(client.request('POST', path, self.request())[0], 429)
            oldest.done.set()
            entered, release = threading.Event(), threading.Event()

            def held(store, current, job):
                entered.set()
                release.wait(5)
                self.complete(store, current, job)

            with patch('hybrid.server.arrangement_jobs.execute', held):
                status, _, active = client.request('POST', path, self.request())
                self.assertEqual(status, 202, active)
                self.assertTrue(entered.wait(2))
                job = state.jobs[active['job_id']]
                try:
                    self.assertIs(state.active_job, job)
                    self.assertEqual(len(state.jobs), MAX_JOB_RECORDS)
                    self.assertIn(oldest.id, state.expired_jobs)
                    self.assertEqual(client.request('GET', path + '/' + job.id)[0], 200)
                    self.assertEqual(client.request('POST', path, self.request())[0], 409)
                    guest, _ = mint_guest(client)
                    self.assertEqual(guest.request('GET', path + '/' + job.id)[0], 404)
                    self.assertEqual(guest.request('POST', path + '/' + job.id + '/cancel', {})[0], 404)
                    self.assertFalse(job.cancel.is_set())
                    self.assertEqual(client.request('POST', path + '/' + job.id + '/cancel', {})[0], 200)
                    self.assertTrue(job.cancel.is_set())
                    self.assertIn(job.id, state.jobs)
                finally:
                    release.set()
                    self.assertTrue(job.done.wait(5))


if __name__ == '__main__':
    unittest.main()
