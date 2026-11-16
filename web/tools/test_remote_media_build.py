"""Qualify the explicit classic-worker bundle, not a general JS transpiler."""
from pathlib import Path
import shutil
import tempfile
import unittest

from build_remote_media_worker import bundle_remote_media_worker


class RemoteMediaBuildTests(unittest.TestCase):
    def test_shared_policy_and_entry_become_one_classic_script(self):
        source = Path(__file__).resolve().parents[1] / 'site'
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            (target / 'hybrid').mkdir()
            for name in ('remote-media-worker.js', 'hybrid/remote-media-policy.js', 'hybrid/endpoint-policy.js'):
                shutil.copy2(source / name, target / name)
            before = (target / 'hybrid/endpoint-policy.js').read_bytes()
            bundle_remote_media_worker(target)
            text = (target / 'remote-media-worker.js').read_text()
            self.assertNotRegex(text, r'(?m)^\s*(?:import|export)\s')
            self.assertIn('function parseMediaUrl', text)
            self.assertIn('function serverEndpoint', text)
            self.assertIn('new Response(response.body', text)
            self.assertEqual(before, (target / 'hybrid/endpoint-policy.js').read_bytes())
            with self.assertRaises(ValueError):
                bundle_remote_media_worker(target)

    def test_new_dependencies_fail_instead_of_being_silently_dropped(self):
        source = Path(__file__).resolve().parents[1] / 'site'
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            shutil.copytree(source / 'hybrid', target / 'hybrid')
            text = (source / 'remote-media-worker.js').read_text() + '\nimport "./new-dependency.js";\n'
            (target / 'remote-media-worker.js').write_text(text)
            with self.assertRaises(ValueError):
                bundle_remote_media_worker(target)
