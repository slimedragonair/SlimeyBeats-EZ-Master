from pathlib import Path
import tempfile
import subprocess
import sys
import json
import unittest
from unittest.mock import patch
from hybrid.deploy.retained_work import inventory, require_empty

class RetainedWorkTests(unittest.TestCase):
    def test_real_prefixed_directory_blocks_even_if_idle_and_other_names_fail_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name in ('.server.lock', '.slimeybeats-hybrid-root'):
                (root / name).write_text('')
            self.assertTrue(inventory(root)['restart_candidate'])
            for name in ('job-' + 'a' * 32, 'b' * 32, 'unrecognized-directory'):
                folder = root / name
                folder.mkdir()
                self.assertEqual(inventory(root)['retained_directories'], 1)
                with self.assertRaises(RuntimeError): require_empty(root)
                folder.rmdir()
            (root / 'unexpected-file').write_text('')
            self.assertFalse(inventory(root)['restart_candidate'])

    def test_cli_blocks_retained_work_without_emitting_private_id(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            private_name = 'job-' + 'c' * 32
            (root / private_name).mkdir()
            cli = Path(__file__).resolve().parents[1] / 'validate_deployment.py'
            command = [sys.executable, str(cli), 'retained-work', '--storage', str(root)]
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 1)
            self.assertEqual(json.loads(result.stdout)['retained_directories'], 1)
            self.assertNotIn(private_name, result.stdout)
            self.assertNotIn(str(root), result.stdout)
            (root / private_name).rmdir()
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0)

    def test_missing_unreadable_and_symlink_roots_never_look_empty(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.assertFalse(inventory(root / 'missing')['inspection_complete'])
            with patch.object(Path, 'iterdir', side_effect=PermissionError):
                self.assertFalse(inventory(root)['restart_candidate'])
            try: (root / 'link').symlink_to(root, target_is_directory=True)
            except OSError: self.skipTest('Symlink privilege unavailable')
            self.assertFalse(inventory(root)['restart_candidate'])
            self.assertFalse(inventory(root / 'link')['inspection_complete'])

if __name__ == '__main__': unittest.main()
