"""Reject connector examples that broaden exposure or weaken the service boundary."""

import contextlib
import json
from pathlib import Path
import re
import shutil
import sys
import tempfile
import unittest

DEPLOY = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(DEPLOY))

from cloudflare_scaffold import check_cloudflare_scaffold
from deployment_scaffold import check_scaffold

EXAMPLES = (
    "slimeybeats-cloudflared.service", "slimeybeats-cloudflared.sysusers.conf",
    "slimeybeats-cloudflared-local.conf.example", "cloudflared-remote.json.example", "cloudflared.yml.example",
)


@contextlib.contextmanager
def examples():
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary)
        for name in EXAMPLES:
            shutil.copyfile(DEPLOY / name, directory / name)
        yield directory


class CloudflareScaffoldTests(unittest.TestCase):
    def test_connector_checks_are_part_of_the_offline_gate(self):
        checks = check_scaffold(DEPLOY)
        self.assertTrue(checks["cloudflare_control_plane_allowed_paths"])
        self.assertTrue(checks["cloudflare_private_token_file"])
        self.assertTrue(all(checks.values()))

    def test_missing_and_malformed_connector_examples_fail_closed(self):
        for name, broken in (("cloudflared-remote.json.example", "{"),
                             ("slimeybeats-cloudflared.service", "[Unit]\nDescription=truncated\n")):
            with self.subTest(name=name), examples() as directory:
                (directory / name).unlink()
                self.assertFalse(all(check_cloudflare_scaffold(directory).values()))
                (directory / name).write_text(broken, encoding="utf-8")
                self.assertFalse(all(check_cloudflare_scaffold(directory).values()))

    def test_service_rejects_privilege_inline_tokens_public_metrics_and_unbounded_resources(self):
        mutations = (
            ("User=slimeybeats-cloudflared", "User=root", "cloudflare_dedicated_account"),
            ("--token-file %d/tunnel-token", "--token secret-in-argv", "cloudflare_private_token_file"),
            ("Environment=HOME=/nonexistent", "Environment=TUNNEL_TOKEN=secret", "cloudflare_no_secret_environment"),
            ("127.0.0.1:20241", "0.0.0.0:20241", "cloudflare_private_token_file"),
            ("MemoryMax=256M", "MemoryMax=infinity", "cloudflare_bounded_resources"),
            ("CPUQuota=100%", "CPUQuota=", "cloudflare_bounded_resources"),
            ("StartLimitBurst=5", "StartLimitBurst=0", "cloudflare_bounded_restarts"),
            ("NoNewPrivileges=yes", "NoNewPrivileges=no", "cloudflare_sandbox"),
            ("CapabilityBoundingSet=", "CapabilityBoundingSet=CAP_NET_ADMIN", "cloudflare_sandbox"),
        )
        for original, replacement, check in mutations:
            with self.subTest(check=check, replacement=replacement), examples() as directory:
                path = directory / "slimeybeats-cloudflared.service"
                path.write_text(path.read_text(encoding="utf-8").replace(original, replacement), encoding="utf-8")
                self.assertFalse(check_cloudflare_scaffold(directory)[check])

    def test_broadened_local_or_remote_ingress_fails(self):
        mutations = (
            ("cloudflared-remote.json.example", '"path": "^/(health|v1/(status|capabilities))$",', ""),
            ("cloudflared-remote.json.example", "api.slimedragon.net", "slimedragon.net"),
            ("cloudflared-remote.json.example", "127.0.0.1:8787", "127.0.0.1:9876"),
            ("cloudflared.yml.example", "path: ^/(health|v1/(status|capabilities))$", "path: ^/v1/.*"),
            ("cloudflared.yml.example", "http_status:404", "http://127.0.0.1:8787"),
        )
        for name, original, replacement in mutations:
            with self.subTest(name=name, replacement=replacement), examples() as directory:
                path = directory / name
                path.write_text(path.read_text(encoding="utf-8").replace(original, replacement), encoding="utf-8")
                self.assertFalse(all(check_cloudflare_scaffold(directory).values()))

    def test_local_alternative_must_clear_remote_credentials_and_command(self):
        for setting in ("LoadCredential=\n", "ExecStartPre=\n", "ExecStart=\n"):
            with self.subTest(setting=setting), examples() as directory:
                path = directory / "slimeybeats-cloudflared-local.conf.example"
                path.write_text(path.read_text(encoding="utf-8").replace(setting, ""), encoding="utf-8")
                self.assertFalse(check_cloudflare_scaffold(directory)["cloudflare_local_mode_clears_remote_credentials"])

    def test_shipped_path_subset_routes_json_only_and_rejects_audio_and_mcp(self):
        ingress = json.loads((DEPLOY / "cloudflared-remote.json.example").read_text(encoding="utf-8"))["config"]["ingress"]

        def route(hostname, path):
            # This pattern uses only the common Go/Python regexp subset; no live tunnel claim.
            return next(rule["service"] for rule in ingress
                        if rule.get("hostname", hostname) == hostname
                        and ("path" not in rule or re.search(rule["path"], path)))

        for path in ("/health", "/v1/status", "/v1/capabilities"):
            self.assertEqual(route("api.slimedragon.net", path), "http://127.0.0.1:8787")
        for path in ("/", "/health/extra", "/v1/jobs", "/v1/session", "/mcp", "/sse",
                     "/v1/jobs/" + "a" * 32, "/v1/jobs/" + "a" * 32 + "/upload",
                     "/v1/jobs/" + "a" * 32 + "/artifacts/" + "b" * 32,
                     "/v1/jobs/download", "/v1/capabilities/file.wav", "/audio.wav"):
            self.assertEqual(route("api.slimedragon.net", path), "http_status:404", path)
        for hostname in ("slimedragon.net", "www.slimedragon.net", "untrusted.invalid"):
            self.assertEqual(route(hostname, "/health"), "http_status:404")


if __name__ == "__main__":
    unittest.main()
