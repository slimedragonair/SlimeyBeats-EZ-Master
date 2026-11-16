"""Regress the missing HTTPS VarRoot and protect the connector's local state."""

from pathlib import Path
import sys
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SOURCE))
from funnel_scaffold import check_unit


class FunnelUnitTests(unittest.TestCase):
    def test_shipped_unit_passes(self):
        checks = check_unit(SOURCE / "slimeybeats-funnel.service")
        self.assertTrue(all(checks.values()), checks)

    def test_state_file_does_not_substitute_for_explicit_statedir(self):
        original = (SOURCE / "slimeybeats-funnel.service").read_text()
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "funnel.service"
            path.write_text(original.replace(" --statedir=/var/lib/slimeybeats-funnel", ""))
            checks = check_unit(path)
            self.assertTrue(checks["stable_node_identity_file"])
            self.assertFalse(checks["explicit_persistent_statedir_for_https"])

    def test_ephemeral_or_foreign_state_and_weaker_privacy_fail(self):
        original = (SOURCE / "slimeybeats-funnel.service").read_text()
        for before, after in (
            ("--statedir=/var/lib/slimeybeats-funnel", "--statedir=/tmp/ephemeral"),
            ("--state=/var/lib/slimeybeats-funnel/tailscaled.state", "--state=mem:"),
            ("RuntimeDirectoryMode=0700", "RuntimeDirectoryMode=0755"),
            ("StateDirectoryMode=0700", "StateDirectoryMode=0755"),
            ("NoNewPrivileges=yes", "NoNewPrivileges=no"),
            ("MemoryMax=384M", "MemoryMax=infinity"),
            ("--no-logs-no-support", "--no-logs-no-support --auth-key=fixture-secret"),
        ):
            with self.subTest(after=after), tempfile.TemporaryDirectory() as temporary:
                path = Path(temporary) / "funnel.service"
                path.write_text(original.replace(before, after))
                self.assertFalse(all(check_unit(path).values()))

    def test_missing_or_truncated_unit_fails_without_traceback(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "missing.service"
            self.assertFalse(all(check_unit(path).values()))
            path.write_text("[Service]\nUser=slimeybeats-funnel\n")
            self.assertFalse(all(check_unit(path).values()))


if __name__ == "__main__":
    unittest.main()
