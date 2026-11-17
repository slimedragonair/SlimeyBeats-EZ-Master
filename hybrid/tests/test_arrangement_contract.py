import unittest
from unittest.mock import patch

from hybrid.server.arrangement_requests import parse
from hybrid.server.errors import ApiError
from hybrid.tests.project_support import project_service, create_project


class ArrangementContractTests(unittest.TestCase):
    def test_metadata_requests_reject_paths_unknown_fields_and_inexact_revisions(self):
        valid = {'operation': 'edit', 'epoch': 'a' * 32, 'expected_revision': 1,
                 'edits': [{'kind': 'set_lane', 'lane_id': 'lane', 'gain_db': -3}]}
        self.assertEqual(parse(valid), valid)
        for value in [dict(valid, path='private.wav'), dict(valid, expected_revision=True),
                      dict(valid, expected_revision=1.5), dict(valid, epoch='../file'),
                      {'operation': 'block', 'frames': 2048},
                      {'operation': 'create', 'channels': True},
                      {'operation': 'add_assets', 'epoch': 'a' * 32, 'expected_revision': 0, 'audio_ids': ['../source.wav']}]:
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
