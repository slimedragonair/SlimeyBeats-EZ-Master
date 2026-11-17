"""Bounded multi-visitor admission preserves retained work and reserves atomically."""
from dataclasses import replace
import threading
import time
import unittest
from unittest.mock import patch

from hybrid.server.arrangement_session import dispatch
from hybrid.server.errors import ApiError
from hybrid.server.principals import OWNER
from hybrid.server.project_limits import arrangement_config, MAX_BANK_PCM_BYTES
from hybrid.tests.project_support import project_service, create_project
from hybrid.tests.support import mint_guest


class FakeNative:
    def __init__(self, *_):
        self.closed = False
        self.value = None

    def call(self, request):
        if request['operation'] == 'create':
            self.value = {'epoch': request['epoch'], 'revision': 0, 'arrangement': {'lanes': []}}
        return self.value

    def close(self):
        self.closed = True


class ArrangementCapacityTests(unittest.TestCase):
    def test_two_guests_retain_independent_work_and_capacity_is_actionable(self):
        with project_service(public_enabled=True, v2_arrangement_enabled=True, v2_arrangement_sessions=2) as (service, owner), patch('hybrid.server.arrangement_session.ArrangementNative', FakeNative):
            first, _ = mint_guest(owner)
            second, _ = mint_guest(owner)
            a, b, c = create_project(first), create_project(second), create_project(owner)
            def create(client, project):
                return client.request('POST', f'/v2/projects/{project["id"]}/arrangement', {'operation': 'create'})
            self.assertEqual(create(first, a)[0], 200)
            self.assertEqual(create(second, b)[0], 200)
            store = service.store.projects
            native_a = store.projects[a['id']].arrangement.native
            native_b = store.projects[b['id']].arrangement.native
            status, headers, result = create(owner, c)
            self.assertEqual((status, result['error']), (429, 'arrangement_capacity'))
            self.assertGreater(int(headers['Retry-After']), 0)
            self.assertFalse(native_a.closed or native_b.closed)
            self.assertEqual(second.request('DELETE', f'/v2/projects/{a["id"]}')[0], 404)
            self.assertEqual(first.request('DELETE', f'/v2/projects/{a["id"]}')[0], 200)
            self.assertTrue(native_a.closed)
            self.assertFalse(native_b.closed)
            self.assertEqual(create(owner, c)[0], 200)

    def test_creation_reservation_is_atomic_and_failed_creation_releases_slot(self):
        with project_service(v2_arrangement_enabled=True, v2_arrangement_sessions=1) as (service, client):
            a, b = create_project(client), create_project(client)
            store = service.store.projects
            started, release = threading.Event(), threading.Event()
            errors = []
            class HeldNative(FakeNative):
                def call(self, request):
                    started.set()
                    release.wait(3)
                    raise RuntimeError('test creation failure')
            def create_a():
                try:
                    dispatch(store, a['id'], {'operation': 'create'}, OWNER)
                except RuntimeError as error:
                    errors.append(error)
            with patch('hybrid.server.arrangement_session.ArrangementNative', HeldNative):
                thread = threading.Thread(target=create_a)
                thread.start()
                try:
                    self.assertTrue(started.wait(2))
                    with self.assertRaises(ApiError) as caught:
                        dispatch(store, b['id'], {'operation': 'create'}, OWNER)
                    self.assertEqual(caught.exception.message, 'arrangement_capacity')
                finally:
                    release.set()
                    thread.join(5)
            self.assertEqual(len(errors), 1)
            self.assertIsNone(store.projects[a['id']].arrangement)
            with patch('hybrid.server.arrangement_session.ArrangementNative', FakeNative):
                self.assertEqual(dispatch(store, b['id'], {'operation': 'create'}, OWNER)['revision'], 0)

    def test_failed_cleanup_keeps_slot_reserved_until_delete_succeeds(self):
        with project_service(v2_arrangement_enabled=True, v2_arrangement_sessions=1) as (service, client):
            a, b = create_project(client), create_project(client)
            store = service.store.projects
            class BrokenNative(FakeNative):
                def call(self, request):
                    raise RuntimeError('test creation failure')
            with patch('hybrid.server.arrangement_session.ArrangementNative', BrokenNative), patch('hybrid.server.arrangement_session.RemoteArrangement.close', side_effect=RuntimeError('cleanup pending')):
                with self.assertRaises(RuntimeError):
                    dispatch(store, a['id'], {'operation': 'create'}, OWNER)
                self.assertIsNotNone(store.projects[a['id']].arrangement)
                with self.assertRaises(ApiError) as caught:
                    dispatch(store, b['id'], {'operation': 'create'}, OWNER)
                self.assertEqual(caught.exception.message, 'arrangement_capacity')
            store.delete(a['id'])
            with patch('hybrid.server.arrangement_session.ArrangementNative', FakeNative):
                self.assertEqual(dispatch(store, b['id'], {'operation': 'create'}, OWNER)['revision'], 0)

    def test_expired_projects_release_capacity_only_through_normal_cleanup(self):
        with project_service(v2_arrangement_enabled=True, v2_arrangement_sessions=1) as (service, client), patch('hybrid.server.arrangement_session.ArrangementNative', FakeNative):
            a, b = create_project(client), create_project(client)
            store = service.store.projects
            dispatch(store, a['id'], {'operation': 'create'}, OWNER)
            native = store.projects[a['id']].arrangement.native
            store.projects[a['id']].deadline = time.monotonic() - 1
            store.expire()
            self.assertTrue(native.closed)
            self.assertEqual(dispatch(store, b['id'], {'operation': 'create'}, OWNER)['revision'], 0)

    def test_configuration_partitions_not_multiplies_memory_and_pcm(self):
        with project_service() as (service, client):
            config = service.config
            for slots in (1, 2, 4, 16):
                changed = replace(config, v2_arrangement_sessions=slots)
                resident = arrangement_config(changed)
                self.assertLessEqual(resident.memory_bytes * slots, config.v2_memory_bytes)
                self.assertLessEqual(resident.max_bank_pcm_bytes * slots, MAX_BANK_PCM_BYTES)
                self.assertLessEqual(resident.max_decoded_bytes, resident.max_bank_pcm_bytes)
            for slots in (0, -1, 17, True, 1.5):
                with self.assertRaises(ValueError):
                    replace(config, v2_arrangement_sessions=slots)
            with self.assertRaises(ValueError):
                replace(config, v2_memory_bytes=1024, v2_arrangement_sessions=4)
            status, _, caps = client.request('GET', '/v2/capabilities')
            self.assertEqual(status, 200)
            self.assertEqual(caps['arrangement']['max_resident_sessions'], 4)


if __name__ == '__main__':
    unittest.main()
