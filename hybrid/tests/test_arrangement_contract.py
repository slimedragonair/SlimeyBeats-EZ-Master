import unittest
from unittest.mock import patch

from hybrid.server.arrangement_requests import parse
from hybrid.server.arrangement_job_requests import request as job_request
from hybrid.server.errors import ApiError
from hybrid.tests.project_support import project_service, create_project


class ArrangementContractTests(unittest.TestCase):
    def test_malformed_operation_scope_and_excluded_id_are_bad_requests(self):
        snapshot = {'epoch': 'a' * 32, 'revision': 1, 'arrangement': {'eq': []}}
        for operation in ([], {}, None, False, 1):
            for decode in (parse, lambda value: job_request(value, snapshot)):
                with self.subTest(operation=operation), self.assertRaises(ApiError) as caught:
                    decode({'operation': operation})
                self.assertEqual(caught.exception.status, 400)
        valid = {'operation': 'suggest_eq', 'epoch': snapshot['epoch'], 'expected_revision': 1,
                 'target': {'scope': {'kind': 'mix'}}}
        for excluded in (False, 0, [], {}, ''):
            with self.subTest(excluded=excluded), self.assertRaises(ApiError) as caught:
                job_request(dict(valid, exclude_eq_id=excluded), snapshot)
            self.assertEqual(caught.exception.status, 400)
        with self.assertRaises(ApiError) as caught:
            job_request(dict(valid, target={'scope': {'kind': []}}), snapshot)
        self.assertEqual(caught.exception.status, 400)

    def test_metadata_requests_reject_paths_unknown_fields_and_inexact_revisions(self):
        valid = {'operation': 'edit', 'epoch': 'a' * 32, 'expected_revision': 1,
                 'edits': [{'kind': 'set_lane', 'lane_id': 'lane', 'gain_db': -3}]}
        self.assertEqual(parse(valid), valid)
        waveform = {'operation': 'waveform', 'epoch': 'a' * 32, 'expected_revision': 1,
                    'asset_id': 'a052b35b-bc81-4e72-9edf-e8b61597f9d2', 'range': {'start_frame': 0, 'end_frame': 1000}, 'bins': 32}
        self.assertEqual(parse(waveform), waveform)
        prepare = {'operation': 'prepare', 'epoch': 'a' * 32, 'expected_revision': 1, 'side': 'after', 'start_frame': 0}
        self.assertEqual(parse(prepare), prepare)
        self.assertEqual(parse(dict(prepare, preview_container='mp4'))['preview_container'], 'mp4')
        for value in [dict(valid, path='private.wav'), dict(valid, expected_revision=True),
                      dict(valid, expected_revision=1.5), dict(valid, epoch='../file'),
                      {'operation': 'block', 'frames': 2048},
                      {'operation': 'create', 'channels': True},
                      {'operation': 'add_assets', 'epoch': 'a' * 32, 'expected_revision': 0, 'audio_ids': ['../source.wav']},
                      dict(waveform, asset_id='../source.wav'), dict(prepare, preview_container='aac')]:
            with self.subTest(value=value), self.assertRaises(ApiError):
                parse(value)

    def test_disabled_feature_never_starts_a_worker_and_keeps_legacy_project(self):
        with project_service() as (_, client):
            project = create_project(client)
            with patch('hybrid.server.arrangement_session.ArrangementNative') as native:
                status, _, value = client.request('POST', f'/v2/projects/{project["id"]}/arrangement', {'operation': 'create'})
                self.assertEqual((status, value['error']), (503, 'arrangement_unavailable'))
                native.assert_not_called()
            status, _, current = client.request('GET', f'/v2/projects/{project["id"]}')
            self.assertEqual((status, current['revision']), (200, project['revision']))
            status, _, caps = client.request('GET', '/v2/capabilities')
            self.assertFalse(caps['arrangement']['available'])
            self.assertFalse(caps['arrangement']['playback'])


if __name__ == '__main__':
    unittest.main()
