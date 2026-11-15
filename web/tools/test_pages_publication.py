import hashlib
import json
import unittest

from pages_artifacts import REPOSITORY
from verify_pages_publication import check_publication, STABLE_FILES, PREVIEW_FILES


class PublicationTests(unittest.TestCase):
    def fixture(self):
        identity = {'branch': 'current-work', 'commit': 'a' * 40, 'main_commit': 'b' * 40,
            'main_run_id': 123, 'runtime_version': '0123456789abcdef'}
        data = {name: ('released:' + name).encode() for name in STABLE_FILES}
        data.update({'current-work/' + name: ('preview:' + name).encode() for name in PREVIEW_FILES})
        data['current-work/preview-build.json'] = json.dumps(identity).encode()
        report = {'repository': REPOSITORY, 'preview': identity,
            'main_files': {name: hashlib.sha256(data[name]).hexdigest() for name in STABLE_FILES},
            'preview_files': {'current-work/' + name: hashlib.sha256(data['current-work/' + name]).hexdigest() for name in PREVIEW_FILES}}
        return report, data

    def test_actual_public_bytes_and_identity_are_required(self):
        report, data = self.fixture()
        result = check_publication(report, lambda name, commit: data[name])
        self.assertTrue(result['passed'])
        for path in ['pkg/dsp_bg.wasm', 'current-work/bootstrap.js', 'current-work/index.html']:
            with self.subTest(path=path):
                changed = {**data, path: b'old or damaged publication'}
                with self.assertRaises(ValueError):
                    check_publication(report, lambda name, commit: changed[name])

    def test_stale_metadata_cannot_validate_new_code(self):
        report, data = self.fixture()
        for field in ['commit', 'main_commit', 'runtime_version']:
            with self.subTest(field=field):
                stale = {**report['preview'], field: '0' * 40}
                changed = {**data, 'current-work/preview-build.json': json.dumps(stale).encode()}
                with self.assertRaises(ValueError):
                    check_publication(report, lambda name, commit: changed[name])


if __name__ == '__main__':
    unittest.main()
