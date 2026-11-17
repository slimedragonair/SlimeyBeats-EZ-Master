"""Archive admission/review quotas and private owned cleanup contracts."""
from pathlib import Path
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from hybrid.server.arrangement_archives import review_result, selection, discard, extraction_budget
from hybrid.server.arrangement_job_requests import request
from hybrid.server.errors import ApiError, ProcessingError
from hybrid.server.project_models import Upload
from hybrid.server.project_requests import upload_request


class ArrangementArchiveTests(unittest.TestCase):
    def test_large_selection_uses_metadata_bytes_and_actual_storage_entries(self):
        choices = [{'part_id': f'{index:032x}', 'version_id': f'{index + 1000:032x}'} for index in range(513)]
        value = {'operation': 'commit_import', 'epoch': 'a' * 32, 'expected_revision': 1,
                 'review_id': 'b' * 32, 'selection': choices}
        snapshot = {'epoch': 'a' * 32, 'revision': 1}
        self.assertEqual(len(request(value, snapshot)['selection']), 513)
        with patch('hybrid.server.arrangement_job_requests.MAX_ARRANGEMENT_IMPORT_METADATA_BYTES', 100), self.assertRaises(ApiError) as caught:
            request(value, snapshot)
        self.assertEqual(caught.exception.message, 'arrangement_import_metadata_limit')
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'existing').write_bytes(b'x')
            project = SimpleNamespace(directory=root, max_disk_bytes=10000, max_storage_files=4096)
            files = [{'bytes': 1} for _ in choices]
            self.assertEqual(extraction_budget(project, files, extra_entries=3), 9999)
            project.max_storage_files = 516
            with self.assertRaises(ApiError) as caught:
                extraction_budget(project, files, extra_entries=3)
            self.assertEqual(caught.exception.message, 'project_file_limit')
            project.max_storage_files, project.max_disk_bytes = 4096, 512
            with self.assertRaises(ApiError) as caught:
                extraction_budget(project, files, extra_entries=3)
            self.assertEqual(caught.exception.message, 'project_disk_limit')

    def test_upload_accepts_large_zip_and_enforces_archive_and_operator_caps(self):
        config = SimpleNamespace(v2_max_upload_bytes=2 * 1024**3)
        value = {'filename': 'Suno.zip', 'role': 'arrangement_archive', 'bytes': 303 * 1024**2}
        self.assertEqual(upload_request(value, config), ('zip', value['bytes'], 'arrangement_archive'))
        for changed, status in [(dict(value, bytes=1024**3 + 1), 413),
                                (dict(value, filename='Suno.wav'), 400), (dict(value, filename='../Suno.zip'), 400)]:
            with self.assertRaises(ApiError) as caught:
                upload_request(changed, config)
            self.assertEqual(caught.exception.status, status)
        config.v2_max_upload_bytes = 128 * 1024**2
        with self.assertRaises(ApiError) as caught:
            upload_request(value, config)
        self.assertEqual(caught.exception.message, 'upload_limit')

    def test_review_count_and_combined_metadata_reject_without_publishing(self):
        state = SimpleNamespace(reviews={}, closed=False, project=SimpleNamespace(deleting=False))
        store = SimpleNamespace(lock=threading.RLock())
        raw = {'parts': [{'name': 'Voice', 'versions': [{'index': 0, 'name': 'Voice.wav', 'format': 'wav',
                                                       'bytes': 44, 'crc32': 0}]}], 'skipped_files': 0}
        def job(index):
            return SimpleNamespace(upload_path=Path('private.zip'), request={'upload_id': str(index)},
                                   cancel=threading.Event(), committed=False, result=None)
        for index in range(4):
            review_result(store, state, job(index), lambda _: raw)
        rejected = job(4)
        with self.assertRaises(ApiError) as caught:
            review_result(store, state, rejected, lambda _: raw)
        self.assertEqual(caught.exception.message, 'arrangement_import_review_limit')
        self.assertEqual(len(state.reviews), 4)
        self.assertFalse(rejected.committed)
        state.reviews.clear()
        with patch('hybrid.server.arrangement_archives.MAX_REVIEW_METADATA_BYTES', 1), self.assertRaises(ApiError):
            review_result(store, state, rejected, lambda _: raw)
        self.assertFalse(state.reviews)
        self.assertIsNone(rejected.result)

    def test_part_version_pairing_and_duplicate_selection_are_guarded(self):
        part_id, version_id, review_id = '1' * 32, '2' * 32, '3' * 32
        state = SimpleNamespace(reviews={review_id: SimpleNamespace(parts={part_id: {'name': 'Voice', 'versions': {version_id: {'index': 0}}}})})
        value = {'operation': 'commit_import', 'epoch': 'a' * 32, 'expected_revision': 1, 'review_id': review_id,
                 'selection': [{'part_id': part_id, 'version_id': version_id}]}
        snapshot = {'epoch': 'a' * 32, 'revision': 1}
        self.assertEqual(selection(state, request(value, snapshot))[1][0]['name'], 'Voice')
        with self.assertRaises(ApiError):
            request(dict(value, selection=value['selection'] * 2), snapshot)
        with self.assertRaises(ApiError):
            selection(state, dict(value, selection=[{'part_id': part_id, 'version_id': '4' * 32}]))
        with self.assertRaises(ApiError) as caught:
            selection(state, dict(value, review_id='5' * 32))
        self.assertEqual(caught.exception.status, 404)

    def test_discard_only_releases_pending_owned_upload(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            owned = root / 'project'
            owned.mkdir()
            upload_id, review_id = '1' * 32, '2' * 32
            file = owned / ('upload-' + upload_id + '.zip')
            file.write_bytes(b'owned archive')
            retained = owned / 'immutable-source.wav'
            retained.write_bytes(b'retained source')
            record = Upload(upload_id, 'zip', 13, 'arrangement_archive', file, uploaded_bytes=13)
            state = SimpleNamespace(project=SimpleNamespace(directory=owned, uploads={upload_id: record}),
                                    reviews={review_id: SimpleNamespace(upload_id=upload_id)})
            self.assertFalse(discard(state, '3' * 32)['discarded'])
            self.assertTrue(file.exists())
            outside = root / file.name
            outside.write_bytes(b'outside')
            record.path = outside
            with self.assertRaises(ProcessingError):
                discard(state, review_id)
            self.assertTrue(outside.exists())
            record.path = file
            self.assertTrue(discard(state, review_id)['discarded'])
            self.assertFalse(file.exists())
            self.assertTrue(retained.exists())
            self.assertFalse(discard(state, review_id)['discarded'])


if __name__ == '__main__':
    unittest.main()
