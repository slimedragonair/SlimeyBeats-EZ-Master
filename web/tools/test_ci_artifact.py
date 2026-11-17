import json
from pathlib import Path
import tempfile
import unittest
from ci_artifact import seal, verify, BINARIES


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root/'web/dist').mkdir(parents=True)
        (self.root/'web/dist/index.html').write_text('test')
        for profile in ('debug', 'release'):
            directory = self.root/'rust/target'/profile
            directory.mkdir(parents=True)
            for name in BINARIES:
                (directory/name).write_bytes(b'native')
        self.manifest = self.root/'manifest.json'
        self.args = (self.root, 'web', 'abc', '123', '1', self.manifest)
        seal(*self.args)

    def test_valid_same_run(self):
        verify(*self.args)

    def test_changed_payload(self):
        (self.root/'web/dist/index.html').write_text('modified')
        with self.assertRaises(ValueError): verify(*self.args)

    def test_added_payload(self):
        (self.root/'web/dist/extra.js').write_text('extra')
        with self.assertRaises(ValueError): verify(*self.args)

    def test_missing_payload(self):
        (self.root/'web/dist/index.html').unlink()
        with self.assertRaises(ValueError): verify(*self.args)

    def test_missing_binary(self):
        (self.root/'rust/target/release'/BINARIES[0]).unlink()
        with self.assertRaises(ValueError): verify(*self.args)

    def test_different_commit_run_attempt_or_kind(self):
        for position, replacement in ((1,'arrangement'), (2,'def'), (3,'999'), (4,'2')):
            args = list(self.args)
            args[position] = replacement
            with self.assertRaises(ValueError): verify(*args)

    def test_symlinks_rejected(self):
        target = self.root/'web/dist/index.html'
        (self.root/'web/dist/link').symlink_to(target)
        with self.assertRaises(ValueError): verify(*self.args)

    def test_symlink_directory_rejected(self):
        (self.root/'web/dist/linkdir').symlink_to(self.root/'rust', target_is_directory=True)
        with self.assertRaises(ValueError): verify(*self.args)

    def test_missing_index_rejected_on_create(self):
        (self.root/'web/dist/index.html').unlink()
        with self.assertRaises(ValueError): seal(*self.args)

    def test_arrangement_profile(self):
        args = (self.root, 'arrangement', 'abc', '123', '1', self.manifest)
        seal(*args)
        verify(*args)
        data = json.loads(self.manifest.read_text())
        self.assertIn('rust/target/debug/'+BINARIES[0], data['files'])


if __name__ == '__main__': unittest.main()
