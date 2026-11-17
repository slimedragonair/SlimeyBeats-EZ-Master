"""Coverage contract for the former serial full-release job."""
from collections import Counter
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from run_browser_shard import commands, run, BROWSERS, GROUPS

CROSS_ENGINE = {
    'verify_recovery_browser.py', 'verify_gapless.py', 'verify_imports.py',
    'verify_browser.py', 'verify_remote_media.py', 'verify_remote_studio.py',
    'verify_remote_migration.py', 'verify_bootstrap_browser.py', 'verify_eq_preview.py',
    'verify_eq_suggestions.py', 'verify_mp3_export.py', 'verify_session_export.py',
    'verify_export_workflows.py', 'verify_studio_browser.py', 'verify_region_edit_browser.py',
    'verify_doctor_browser.py', 'verify_studio_live.py', 'verify_learning_browser.py',
}
ONCE = {'verify_session_export_lifecycle.py', 'verify_memory.py', 'verify_studio_memory.py'}


class ShardTests(unittest.TestCase):
    def test_every_original_suite_once_per_engine(self):
        inventory = Counter()
        for browser in BROWSERS:
            for group in GROUPS:
                for cmd in commands(browser, group, Path('/reports')):
                    name = Path(cmd[1]).name
                    if name in CROSS_ENGINE:
                        self.assertIn('--browser', cmd)
                        self.assertEqual(cmd[cmd.index('--browser')+1], browser)
                        inventory[(name, browser)] += 1
                    else:
                        self.assertIn(name, ONCE)
                        inventory[(name, None)] += 1
        expected = Counter({(name, browser): 1 for name in CROSS_ENGINE for browser in BROWSERS})
        expected.update({(name, None): 1 for name in ONCE})
        self.assertEqual(inventory, expected)

    def test_critical_original_arguments_preserved(self):
        all_commands = [cmd for b in BROWSERS for g in GROUPS for cmd in commands(b, g, Path('/reports'))]
        for cmd in all_commands:
            name = Path(cmd[1]).name
            if name == 'verify_mp3_export.py': self.assertIn('--integration', cmd)
            if name in ('verify_memory.py','verify_studio_memory.py'):
                self.assertEqual(cmd[cmd.index('--tracks')+1], '4')
                self.assertEqual(cmd[cmd.index('--seconds')+1], '10')
            if name == 'verify_studio_memory.py': self.assertIn('--check-limits', cmd)
            if name.startswith('verify_remote_'):
                self.assertEqual(cmd[cmd.index('--cli')+1], 'rust/target/release/slimeybeats-ez-master-cli')
                self.assertEqual(cmd[cmd.index('--worker')+1], 'rust/target/release/slimeybeats_hybrid_worker')

    def test_aggregate_only_skips_an_explicit_successful_out_of_scope_result(self):
        root = Path(__file__).resolve().parents[2]
        for filename, scope in [('web.yml', 'web'), ('arrangement.yml', 'arrangement')]:
            workflow = (root/'.github/workflows'/filename).read_text()
            self.assertIn(f"if: always() && (needs.scope.result != 'success' || needs.scope.outputs.{scope} != 'false')", workflow)
            for result in ['success', 'failure', 'cancelled', 'skipped']:
                for output in ['true', 'false', '', 'unexpected']:
                    should_run = result != 'success' or output != 'false'
                    self.assertEqual(not should_run, result == 'success' and output == 'false')

    def test_native_stream_runs_on_its_original_default_engine(self):
        root = Path(__file__).resolve().parents[2]
        workflow = (root/'.github/workflows/arrangement.yml').read_text()
        step = workflow.split('- name: Verify native MP4 decoding, bounded reads and owned cancellation', 1)[1].split('- uses:', 1)[0]
        self.assertIn("if: matrix.browser == 'webkit'", step)
        source = (root/'web/tools/verify_arrangement_stream_browser.py').read_text()
        self.assertIn("choices=['chromium', 'firefox', 'webkit'], default='webkit'", source)

    def test_failure_stops_shard_and_is_recorded(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            with patch('run_browser_shard.subprocess.run') as child:
                child.return_value.returncode = 9
                with self.assertRaises(SystemExit) as error: run('chromium','core',out)
                self.assertEqual(error.exception.code, 9)
                self.assertEqual(child.call_count, 1)
                self.assertIn('"returncode": 9', (out/'shard-timings.jsonl').read_text())

    def test_invalid_shard_rejected(self):
        with self.assertRaises(ValueError): commands('missing','core',Path('.'))
        with self.assertRaises(ValueError): commands('chromium','missing',Path('.'))


if __name__ == '__main__': unittest.main()
