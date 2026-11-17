"""Offline-only owner drain regression: never touches deployed service."""
import threading
import unittest
from hybrid.tests.project_support import project_service, create_project, upload
from hybrid.tests.support import mint_guest, wav_bytes

ROUTE = '/v1/maintenance/drain'

class MaintenanceTests(unittest.TestCase):
    def test_owner_only_gate_preserves_existing_work_and_is_reversible(self):
        with project_service(public_enabled=True) as (service, owner):
            guest, _ = mint_guest(owner)
            project = create_project(guest)
            status, _, job = owner.create()
            self.assertEqual(status, 201)
            self.assertEqual(guest.request('POST', ROUTE, {'enabled': True})[0], 403)
            self.assertEqual(owner.request('POST', ROUTE, {'enabled': 1})[0], 400)
            status, _, state = owner.request('POST', ROUTE, {'enabled': True})
            self.assertEqual((status, state['retained_projects'], state['retained_jobs']), (200, 1, 1))
            self.assertEqual(owner.request('GET', ROUTE, auth=False)[0], 401)
            for client in (guest, owner):
                response = client.request('POST', '/v2/projects', {})
                self.assertEqual((response[0], response[2]['error']), (503, 'maintenance_draining'))
                self.assertEqual(response[1]['Retry-After'], '60')
                self.assertEqual(client.create()[0], 503)
            self.assertEqual(owner.request('POST', '/v1/session', {}, auth=False, origin='https://owner.example')[0], 503)
            self.assertEqual(guest.request('GET', '/v2/projects/' + project['id'])[0], 200)
            upload(guest, project['id'], wav_bytes())
            owner.upload(job['id'], wav_bytes())
            self.assertEqual(owner.request('GET', '/v1/jobs/' + job['id'])[0], 200)
            self.assertEqual(owner.request('POST', ROUTE, {'enabled': False})[0], 200)
            self.assertEqual(owner.request('POST', '/v2/projects', {})[0], 201)

    def test_create_racing_gate_is_either_counted_or_rejected(self):
        with project_service() as (service, owner):
            go = threading.Event()
            results = []
            def create():
                go.wait()
                results.append(owner.request('POST', '/v2/projects', {})[0])
            thread = threading.Thread(target=create)
            thread.start()
            with service.store.lock:
                go.set()
                state = service.store.maintenance(True)
                self.assertEqual(state['retained_projects'], 0)
            thread.join(5)
            self.assertEqual(results, [503])
            self.assertEqual(service.store.maintenance()['retained_projects'], 0)
