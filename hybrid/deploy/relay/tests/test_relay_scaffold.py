"""Reject broadened listeners, spoofable headers and weaker SSH boundaries."""

import contextlib
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

RELAY = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(RELAY))
from relay_scaffold import check_scaffold


@contextlib.contextmanager
def examples():
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary)
        for path in RELAY.iterdir():
            if path.is_file() and path.suffix in (".example", ".service", ".conf"):
                shutil.copyfile(path, directory / path.name)
        yield directory


class RelayScaffoldTests(unittest.TestCase):
    def test_shipped_examples_pass(self):
        checks = check_scaffold(RELAY)
        self.assertGreater(len(checks), 10)
        self.assertTrue(all(checks.values()), checks)

    def test_missing_empty_or_duplicate_sources_fail_without_traceback(self):
        for name in ("Caddyfile.example", "home-ssh.conf.example", "vps-sshd.conf.example", "slimeybeats-relay.service"):
            with self.subTest(name=name), examples() as directory:
                path = directory / name
                original = path.read_text()
                path.unlink()
                self.assertFalse(all(check_scaffold(directory).values()))
                path.write_text("")
                self.assertFalse(all(check_scaffold(directory).values()))
                path.write_text(original + original)
                self.assertFalse(all(check_scaffold(directory).values()))

    def test_ssh_client_rejects_unpinned_or_public_listener_and_extra_forward(self):
        changes = (
            ("StrictHostKeyChecking yes", "StrictHostKeyChecking accept-new"),
            ("127.0.0.1:18787", "0.0.0.0:18787"),
            ("ServerAliveCountMax 3", "ServerAliveCountMax 0"),
            ("ExitOnForwardFailure yes", "ExitOnForwardFailure no"),
            ("IdentityAgent none", "IdentityAgent SSH_AUTH_SOCK"),
            ("ControlMaster no", "ControlMaster auto"),
            ("Port 22", "Port 22\n    LocalForward 9999 127.0.0.1:22"),
        )
        for original, replacement in changes:
            with self.subTest(replacement=replacement), examples() as directory:
                path = directory / "home-ssh.conf.example"
                path.write_text(path.read_text().replace(original, replacement))
                self.assertFalse(check_scaffold(directory)["ssh_pinned_host_and_loopback_forward"])

    def test_sshd_rejects_shell_auth_and_forwarding_expansion(self):
        for original, replacement in (
            ("Match User slimeybeats-tunnel", "Match all"),
            ("AllowTcpForwarding remote", "AllowTcpForwarding yes"),
            ("PermitListen 127.0.0.1:18787", "PermitListen any"),
            ("GatewayPorts no", "GatewayPorts clientspecified"),
            ("PasswordAuthentication no", "PasswordAuthentication yes"),
            ("MaxSessions 0", "MaxSessions 1"),
            ("ForceCommand /bin/false", "ForceCommand /bin/sh"),
            ("PermitUserRC no", "PermitUserRC yes"),
        ):
            with self.subTest(replacement=replacement), examples() as directory:
                path = directory / "vps-sshd.conf.example"
                path.write_text(path.read_text().replace(original, replacement))
                self.assertFalse(check_scaffold(directory)["sshd_remote_only_dedicated_match"])

    def test_caddy_rejects_spoofed_ip_upstream_exposure_mcp_and_unrestricted_route(self):
        for original, replacement in (
            ("{remote_host}", "{header.X-SlimeyBeats-Client-IP}"),
            ("127.0.0.1:18787", "127.0.0.1:8787"),
            ("header_up -CF-*", "header_up CF-Connecting-IP {remote_host}"),
            ("header_up -X-Forwarded-*", ""),
            ("@api path /health", "@api path /mcp /health"),
            ("@api path /health /v1/status /v1/session /v1/capabilities /v1/jobs /v1/jobs/*", "@api path /*"),
            ("admin 127.0.0.1:2019", "admin :2019"),
        ):
            with self.subTest(replacement=replacement), examples() as directory:
                path = directory / "Caddyfile.example"
                path.write_text(path.read_text().replace(original, replacement))
                self.assertFalse(check_scaffold(directory)["caddy_fixed_api_and_peer_header_policy"])

    def test_private_key_pin_and_systemd_protections_are_required(self):
        for name, original, replacement in (
            ("slimeybeats-relay.service", "-i %d/relay-key", "-i /home/shared/key"),
            ("slimeybeats-relay.service", "MemoryMax=64M", "MemoryMax=infinity"),
            ("slimeybeats-relay.service", "NoNewPrivileges=yes", "NoNewPrivileges=no"),
            ("home-backend-localonly.conf.example", "IPAddressDeny=any", "IPAddressDeny="),
            ("home-backend-localonly.conf.example", "UnsetEnvironment=SLIMEYBEATS_HYBRID_PUBLIC", ""),
            ("vps-authorized_keys.example", "restrict,port-forwarding,", ""),
            ("home-known_hosts.example", "ssh-ed25519", "ssh-rsa"),
            ("vps-caddy-limits.conf.example", "MemoryMax=256M", "MemoryMax=infinity"),
        ):
            with self.subTest(name=name, replacement=replacement), examples() as directory:
                path = directory / name
                path.write_text(path.read_text().replace(original, replacement))
                self.assertFalse(all(check_scaffold(directory).values()))


if __name__ == "__main__":
    unittest.main()
