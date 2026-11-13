"""Fast cache/extraction regressions; no model download, TensorFlow or network required."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parents[1] / "src/studio_native/model_install.py"
spec = importlib.util.spec_from_file_location("model_installer", SOURCE)
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class ModelInstallTests(unittest.TestCase):
    def archive(self, root, extra=None):
        archive = root / "model.tar.gz"
        files = {"./._checkpoint": b"resource fork", "checkpoint": b'model_checkpoint_path: "model"',
                 "model.index": b"index", "model.data-00000-of-00001": b"weights"}
        files.update(extra or {})
        with tarfile.open(archive, "w:gz") as bundle:
            for name, content in files.items():
                info = tarfile.TarInfo(name)
                info.size = len(content)
                bundle.addfile(info, io.BytesIO(content))
        installer.SHA256 = hashlib.sha256(archive.read_bytes()).hexdigest()
        return archive

    def test_resource_fork_is_ignored_and_repeated_setup_recovers(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.archive(root)
            target = root / "models/4stems"
            for _ in range(3):
                installer.install(archive, target)
                self.assertTrue((target / "model.index").is_file())
                self.assertFalse((target / "._checkpoint").exists())
            self.assertEqual(list(target.parent.glob("*.previous-*")), [])

    def test_unknown_cache_files_are_preserved_when_repairing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.archive(root)
            target = root / "models/4stems"
            installer.install(archive, target)
            (target / "my-notes.txt").write_text("keep me")
            installer.install(archive, target)
            backups = list(target.parent.glob("4stems.previous-*"))
            self.assertEqual(len(backups), 1)
            self.assertEqual((backups[0] / "my-notes.txt").read_text(), "keep me")

    def test_bad_hash_and_traversal_cannot_replace_a_working_cache(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.archive(root)
            target = root / "models/4stems"
            installer.install(archive, target)
            before = (target / "model.index").read_bytes()
            archive.write_bytes(b"broken")
            with self.assertRaises(ValueError):
                installer.install(archive, target)
            self.assertEqual(before, (target / "model.index").read_bytes())
            archive = self.archive(root, {"../outside.txt": b"escape"})
            with self.assertRaises(ValueError):
                installer.install(archive, target)
            self.assertFalse((root / "outside.txt").exists())


if __name__ == "__main__":
    unittest.main()
