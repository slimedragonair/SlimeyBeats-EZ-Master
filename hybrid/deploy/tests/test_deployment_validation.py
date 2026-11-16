"""Local fixtures verify secret handling, transport boundaries and capacity gates."""

import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch
from types import SimpleNamespace

DEPLOY = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(DEPLOY))

from deployment_health import EXPECTED_LIMITS, PAGES_ORIGIN, check_health
from deployment_inventory import _codec_ready, check_inventory
from deployment_scaffold import check_scaffold
from validate_deployment import main

TOKEN = "fixture-secret-for-read-only-checks-123456789"


class HealthFixture(BaseHTTPRequestHandler):
    mode = "healthy"
    requests = []

    def log_message(self, *args):
        pass

    def send_document(self, code, value, cors=True):
        self.send_response(code)
        if cors:
            self.send_header("Access-Control-Allow-Origin", PAGES_ORIGIN)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(value).encode())

    def do_GET(self):
        self.requests.append((self.command, self.path, self.headers.get("Authorization")))
        if self.mode == "redirect":
            self.send_response(307)
            self.send_header("Location", self.server.base_url + "/redirect-target")
            self.end_headers()
        elif self.path == "/health":
            self.send_document(200, {"status": "ok"})
        elif self.headers.get("Authorization") != "Bearer " + TOKEN:
            self.send_document(401, {"error": "do not log " + TOKEN})
        elif self.mode == "malformed":
            self.send_document(200, {"formats": [{}], "operations": None, "limits": [], "profiles": 1})
        elif self.mode == "oversized":
            self.send_document(200, {"padding": "x" * 70000})
        elif self.mode == "broken-chunk":
            self.send_response(200)
            self.send_header("Transfer-Encoding", "chunked")
            self.end_headers()
            self.wfile.write(TOKEN.encode() + b"\r\n")
            self.close_connection = True
        else:
            self.send_document(200, {
                "api_version": 1, "operations": ["master"], "formats": ["wav24", "mp3"],
                "profiles": [{"id": "neutral", "label": "Neutral"}], "limits": EXPECTED_LIMITS,
                "privacy": {"temporary_storage": True, "retention_seconds": 3600},
            })

    def do_OPTIONS(self):
        self.requests.append((self.command, self.path, self.headers.get("Authorization")))
        if self.headers.get("Origin") != PAGES_ORIGIN:
            self.send_document(403, {}, cors=False)
            return
        self.send_response(204)
        self.send_header("Access-Control-Allow-Origin", PAGES_ORIGIN)
        self.send_header("Access-Control-Allow-Headers", "Authorization, Content-Type")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
        self.end_headers()


@contextlib.contextmanager
def server(mode="healthy"):
    fixture = type("Fixture", (HealthFixture,), {"mode": mode, "requests": []})
    httpd = ThreadingHTTPServer(("127.0.0.1", 0), fixture)
    httpd.base_url = "http://127.0.0.1:" + str(httpd.server_port)
    worker = threading.Thread(target=httpd.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True)
    worker.start()
    try:
        yield httpd.base_url, fixture.requests
    finally:
        httpd.shutdown()
        httpd.server_close()
        worker.join()


class HealthTests(unittest.TestCase):
    def test_healthy_probe_is_read_only_and_sends_bearer_only_to_capabilities(self):
        with server() as (endpoint, requests):
            self.assertTrue(all(check_health(endpoint, TOKEN).values()))
        self.assertTrue(all(method in ("GET", "OPTIONS") for method, _, _ in requests))
        authenticated = [(method, path) for method, path, auth in requests if auth]
        self.assertEqual(authenticated, [("GET", "/v1/capabilities")])

    def test_tokens_and_response_bodies_never_reach_cli_output(self):
        with server() as (endpoint, _), patch.dict(os.environ, {"SLIMEYBEATS_HYBRID_TOKEN": TOKEN}):
            output = io.StringIO()
            with patch.object(sys, "argv", ["check", "health", "--endpoint", endpoint]), contextlib.redirect_stdout(output):
                self.assertEqual(main(), 0)
        self.assertNotIn(TOKEN, output.getvalue())
        self.assertNotIn(endpoint, output.getvalue())
        self.assertNotIn("do not log", output.getvalue())
        self.assertTrue(json.loads(output.getvalue())["passed"])

    def test_redirects_are_never_followed(self):
        with server("redirect") as (endpoint, requests):
            self.assertFalse(all(check_health(endpoint, TOKEN).values()))
        self.assertFalse(any(path == "/redirect-target" for _, path, _ in requests))

    def test_malformed_and_oversized_capabilities_fail_without_traceback(self):
        for mode in ("malformed", "oversized", "broken-chunk"):
            with self.subTest(mode=mode), server(mode) as (endpoint, _):
                self.assertFalse(all(check_health(endpoint, TOKEN).values()))

    def test_unapproved_endpoints_and_unsafe_tokens_send_nothing(self):
        endpoints = ("http://api.slimedragon.net", "https://example.invalid", "https://api.slimedragon.net/?token=secret",
                     "https://user@api.slimedragon.net", "https://api.slimedragon.net:444", "http://[::1]:8787")
        with patch("urllib.request.build_opener") as opener:
            for endpoint in endpoints:
                self.assertFalse(all(check_health(endpoint, TOKEN).values()))
            for token in ("short", TOKEN + "\nHeader: value", TOKEN + "\u2603"):
                self.assertFalse(all(check_health("http://127.0.0.1:8787", token).values()))
            opener.assert_not_called()


class InventoryTests(unittest.TestCase):
    def test_missing_tunnel_is_optional_for_loopback(self):
        with patch("deployment_inventory.shutil.which", return_value=None):
            self.assertNotIn("cloudflared_available", check_inventory(None, None))
            self.assertFalse(check_inventory(None, None, True)["cloudflared_available"])

    def test_root_volume_and_nearly_full_volume_fail_even_when_writable(self):
        with tempfile.TemporaryDirectory() as directory:
            storage = Path(directory)
            with patch("deployment_inventory.shutil.disk_usage", return_value=SimpleNamespace(total=100, used=94, free=6)), \
                    patch("deployment_inventory._codec_ready", return_value=True):
                checks = check_inventory(storage / "missing-cli", storage)
            self.assertFalse(checks["storage_at_least_16_gib_free"])
            self.assertFalse(checks["storage_volume_at_most_85_percent_used"])
            if storage.stat().st_dev == Path("/").stat().st_dev:
                self.assertFalse(checks["storage_on_non_root_volume"])

    def test_inventory_does_not_pass_bearer_to_codec_probes(self):
        with patch.dict(os.environ, {"SLIMEYBEATS_HYBRID_TOKEN": TOKEN}), \
                patch("deployment_inventory.shutil.which", return_value="/usr/bin/ffmpeg"), \
                patch("deployment_inventory.subprocess.run", return_value=SimpleNamespace(returncode=0)) as run:
            self.assertTrue(_codec_ready("ffmpeg"))
        self.assertNotIn("SLIMEYBEATS_HYBRID_TOKEN", run.call_args.kwargs["env"])
        self.assertFalse(run.call_args.kwargs.get("shell", False))


class ScaffoldTests(unittest.TestCase):
    def test_source_examples_are_consistent(self):
        self.assertTrue(all(check_scaffold(DEPLOY).values()))

    def test_missing_source_examples_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            self.assertFalse(all(check_scaffold(Path(directory)).values()))

    def test_truncated_unit_fails_without_traceback(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            (directory / "slimeybeats-hybrid.service").write_text("[Service]\nUser=somebody\n", encoding="utf-8")
            for name in ("slimeybeats-hybrid.env.example", "cloudflared.yml.example"):
                (directory / name).write_bytes((DEPLOY / name).read_bytes())
            self.assertFalse(all(check_scaffold(directory).values()))


if __name__ == "__main__":
    unittest.main()
