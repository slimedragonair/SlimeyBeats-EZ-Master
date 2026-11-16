"""Tool checks stay offline, suppress raw errors, and validate effective policy."""

import contextlib
import io
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

RELAY = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(RELAY))
from relay_tool_checks import check_caddy, check_ssh
from validate_relay import main


def caddy_document():
    return {"admin": {"listen": "127.0.0.1:2019"}, "routes": [{
        "match": [{"host": ["api.slimedragon.net"]}], "handle": [{
            "handler": "reverse_proxy", "upstreams": [{"dial": "127.0.0.1:18787"}],
            "headers": {"request": {
                "set": {"Host": ["api.slimedragon.net"], "X-SlimeyBeats-Client-IP": ["{http.request.remote.host}"]},
                "delete": ["CF-*", "X-Forwarded-*", "Forwarded", "X-Real-IP"],
            }},
        }],
    }]}


class RelayToolTests(unittest.TestCase):
    def test_offline_default_reports_native_and_live_checks_not_run(self):
        output = io.StringIO()
        with patch.object(sys, "argv", ["check"]), contextlib.redirect_stdout(output), \
                patch("relay_tool_checks.subprocess.run") as run:
            self.assertEqual(main(), 0)
        run.assert_not_called()
        document = json.loads(output.getvalue())
        self.assertTrue(document["passed"])
        self.assertTrue(all(item["status"] == "not_run" for item in document["tools"].values()))

    def test_caddy_adapts_only_and_never_inherits_credentials_or_starts_server(self):
        with patch("relay_tool_checks.subprocess.run") as run, \
                patch.dict(os.environ, {"SLIMEYBEATS_HYBRID_TOKEN": "fixture-secret", "OCI_AUTH_TOKEN": "private",
                                        "USERPROFILE": "fixture-profile"}):
            run.return_value = SimpleNamespace(returncode=0, stdout=json.dumps(caddy_document()).encode())
            self.assertEqual(check_caddy(Path("caddy"), RELAY)["status"], "passed")
        arguments = run.call_args.args[0]
        self.assertEqual(arguments[1], "adapt")
        self.assertNotIn("run", arguments)
        self.assertFalse(run.call_args.kwargs["shell"])
        self.assertNotIn("SLIMEYBEATS_HYBRID_TOKEN", run.call_args.kwargs["env"])
        self.assertNotIn("OCI_AUTH_TOKEN", run.call_args.kwargs["env"])
        self.assertEqual(run.call_args.kwargs["env"]["USERPROFILE"], "fixture-profile")

    def test_native_caddy_policy_rejects_different_header_or_extra_upstream(self):
        for mutation in ("header", "upstream", "trusted", "missing_delete"):
            document = caddy_document()
            proxy = document["routes"][0]["handle"][0]
            if mutation == "header":
                proxy["headers"]["request"]["set"]["X-SlimeyBeats-Client-IP"] = ["{http.request.header.X-Forwarded-For}"]
            elif mutation == "upstream":
                proxy["upstreams"].append({"dial": "public.invalid:8787"})
            elif mutation == "trusted":
                document["trusted_proxies"] = {"source": "static", "ranges": ["0.0.0.0/0"]}
            else:
                proxy["headers"]["request"]["delete"].remove("X-Forwarded-*")
            with self.subTest(mutation=mutation), patch("relay_tool_checks.subprocess.run") as run:
                run.return_value = SimpleNamespace(returncode=0, stdout=json.dumps(document).encode())
                self.assertEqual(check_caddy(Path("caddy"), RELAY)["status"], "failed")

    def test_failed_tools_do_not_print_raw_errors(self):
        output = io.StringIO()
        with patch("relay_tool_checks.subprocess.run") as run, contextlib.redirect_stdout(output):
            run.return_value = SimpleNamespace(returncode=1, stdout=b"fixture-private-error", stderr=b"fixture-private-error")
            self.assertEqual(check_caddy(Path("caddy"), RELAY)["status"], "failed")
            self.assertEqual(check_ssh(Path("ssh"), RELAY)["status"], "failed")
        self.assertEqual(output.getvalue(), "")

    def test_ssh_inspects_configuration_without_connecting(self):
        with patch("relay_tool_checks.subprocess.run") as run:
            run.return_value = SimpleNamespace(returncode=1, stdout=b"")
            self.assertEqual(check_ssh(Path("ssh"), RELAY)["status"], "failed")
        args = run.call_args.args[0]
        self.assertEqual(args[1], "-G")
        self.assertNotIn("-R", args)


if __name__ == "__main__":
    unittest.main()
