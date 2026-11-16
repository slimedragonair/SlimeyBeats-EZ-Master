"""Explicit relay identity trust across a protected loopback forwarding boundary."""

from contextlib import redirect_stderr
from email.message import Message
import http.client
import io
import os
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from hybrid.server.client_identity import client_key
from hybrid.server.config import Config, from_arguments
from hybrid.server.errors import ApiError
from hybrid.tests.support import ORIGIN, TOKEN, mint_guest, running_service

HOST = "api.slimedragon.net"
RELAY_HEADER = "X-SlimeyBeats-Client-IP"


class RelayClientIdentityTests(unittest.TestCase):
    def test_flags_are_mutually_exclusive_and_relay_configuration_fails_closed(self):
        defaults = dict(cli=Path(sys.executable), token=TOKEN)
        for extra in ({"trust_relay_client_ip": True},
                      {"trust_relay_client_ip": True, "expected_host": "evil.example"},
                      {"trust_relay_client_ip": True, "expected_host": HOST, "host": "0.0.0.0"},
                      {"trust_relay_client_ip": True, "expected_host": HOST, "host": "192.0.2.1"},
                      {"trust_relay_client_ip": True, "expected_host": HOST, "host": "::1"},
                      {"trust_relay_client_ip": 1, "expected_host": HOST},
                      {"trust_relay_client_ip": True, "trust_cloudflare_client_ip": True,
                       "expected_host": HOST}):
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                Config(**defaults, **extra)
        with patch.dict(os.environ, {"SLIMEYBEATS_HYBRID_TOKEN": TOKEN,
                                     "SLIMEYBEATS_HYBRID_PUBLIC": "0"}):
            config = from_arguments(["--cli", sys.executable, "--trust-relay-client-ip", "--expected-host", HOST])
            self.assertTrue(config.trust_relay_client_ip)
            self.assertFalse(config.trust_cloudflare_client_ip)
            self.assertFalse(config.public_enabled)
            self.assertEqual((config.host, config.port), ("127.0.0.1", 8787))
            for flags in (("--trust-relay-client-ip", "--trust-cloudflare-client-ip"),
                          ("--trust-cloudflare-client-ip", "--trust-relay-client-ip")):
                with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as caught:
                    from_arguments(["--cli", sys.executable, "--expected-host", HOST, *flags])
                self.assertEqual(caught.exception.code, 2)

    def test_relay_spoofs_are_ignored_by_default(self):
        with running_service(public_enabled=True) as (service, owner):
            for index in range(4):
                headers = {RELAY_HEADER: f"203.0.113.{index}", "CF-Connecting-IP": "bad",
                           "X-Forwarded-For": f"198.51.100.{index}"}
                self.assertEqual(owner.request("POST", "/v1/session", {}, auth=False,
                                              origin=ORIGIN, headers=headers)[0], 201)
            status, headers, _ = owner.request("POST", "/v1/session", {}, auth=False,
                                               origin=ORIGIN, headers={RELAY_HEADER: "192.0.2.1"})
            self.assertEqual(status, 429)
            self.assertGreater(int(headers["Retry-After"]), 0)
            self.assertEqual(len(service.store.sessions._rates), 1)
            peer_key = service.store.sessions.client_key("127.0.0.1")
            self.assertEqual(set(service.store.sessions._rates), {("session", peer_key)})

    def test_relay_uses_valid_selected_header_for_sessions_and_job_starts(self):
        with running_service(public_enabled=True, expected_host=HOST,
                             trust_relay_client_ip=True) as (service, owner):
            addresses = ["203.0.113.1", "2001:db8::1"]
            for address in addresses:
                headers = {"Host": HOST, RELAY_HEADER: address, "CF-Connecting-IP": "bad",
                           "X-Forwarded-For": "198.51.100.1, 198.51.100.2"}
                guest, _ = mint_guest(owner, headers=headers)
                self.assertIn(("session", service.store.sessions.client_key(address)), service.store.sessions._rates)
                service.worker.process = lambda job: {}
                job = guest.create(b"x")[2]["id"]
                guest.upload(job, b"x")
                self.assertEqual(guest.request("POST", f"/v1/jobs/{job}/run", {},
                                               headers={RELAY_HEADER: ""})[0], 400)
                self.assertEqual(guest.request("POST", f"/v1/jobs/{job}/run", {})[0], 200)
                self.assertIn(("job", service.store.sessions.client_key(address)), service.store.sessions._rates)
                self.assertEqual(guest.request("DELETE", "/v1/jobs/" + job)[0], 200)
            self.assertNotIn("203.0.113", repr(service.store.sessions._rates))
            self.assertNotIn("2001:db8", repr(service.store.sessions._rates))

    def test_relay_rejects_wrong_host_missing_header_and_invalid_addresses(self):
        with running_service(public_enabled=True, expected_host=HOST,
                             trust_relay_client_ip=True) as (service, owner):
            rejected = [{}, {"Host": HOST}, {"Host": "evil.example", RELAY_HEADER: "203.0.113.1"},
                        {"Host": HOST, "CF-Connecting-IP": "203.0.113.1"}]
            rejected += [{"Host": HOST, RELAY_HEADER: value} for value in (
                "", "bad", "203.0.113.1, 203.0.113.2", "203.0.113.1:443", "[2001:db8::1]",
                "fe80::1%eth0", "fe80::1%2", "203.0.113.1%eth0", "2001:db8::1 ")]
            for headers in rejected:
                with self.subTest(headers=headers):
                    self.assertEqual(owner.request("POST", "/v1/session", {}, auth=False,
                                                  origin=ORIGIN, headers=headers)[0], 400)
            self.assertEqual(service.store.sessions._sessions, {})
            self.assertEqual(service.store.sessions._rates, {})

    def test_relay_rejects_duplicate_selected_headers_and_nonloopback_peers(self):
        with running_service(public_enabled=True, expected_host=HOST,
                             trust_relay_client_ip=True) as (service, owner):
            connection = http.client.HTTPConnection("127.0.0.1", owner.port, timeout=3)
            try:
                connection.putrequest("POST", "/v1/session", skip_host=True)
                for name, value in (("Host", HOST), ("Origin", ORIGIN),
                                    ("Content-Type", "application/json"), ("Content-Length", "2"),
                                    (RELAY_HEADER, "203.0.113.1"), (RELAY_HEADER.lower(), "203.0.113.1")):
                    connection.putheader(name, value)
                connection.endheaders(b"{}")
                response = connection.getresponse()
                self.assertEqual(response.status, 400)
                response.read()
            finally:
                connection.close()
            headers = Message()
            headers["Host"] = HOST
            headers[RELAY_HEADER] = "203.0.113.1"
            handler = SimpleNamespace(headers=headers, server=service.http, client_address=("198.51.100.1", 123))
            with self.assertRaises(ApiError) as caught:
                client_key(handler)
            self.assertEqual(caught.exception.status, 400)
            handler.client_address = ("127.0.0.1", 123)
            self.assertEqual(client_key(handler), service.store.sessions.client_key("203.0.113.1"))
            headers["Host"] = HOST  # A duplicate Host is rejected even when both values match.
            with self.assertRaises(ApiError):
                client_key(handler)
            self.assertEqual(service.store.sessions._sessions, {})

    def test_modes_ignore_unselected_headers_and_never_fall_back_to_them(self):
        for flag, selected, other in (("trust_relay_client_ip", RELAY_HEADER, "CF-Connecting-IP"),
                                     ("trust_cloudflare_client_ip", "CF-Connecting-IP", RELAY_HEADER)):
            with self.subTest(flag=flag), running_service(public_enabled=True, expected_host=HOST,
                                                          **{flag: True}) as (service, owner):
                self.assertEqual(owner.request("POST", "/v1/session", {}, auth=False, origin=ORIGIN,
                                              headers={"Host": HOST, other: "203.0.113.1"})[0], 400)
                for index in range(4):
                    headers = {"Host": HOST, selected: "203.0.113.1", other: f"198.51.100.{index}",
                               "X-Forwarded-For": f"192.0.2.{index}"}
                    self.assertEqual(owner.request("POST", "/v1/session", {}, auth=False,
                                                  origin=ORIGIN, headers=headers)[0], 201)
                headers[other] = "198.51.100.100"
                self.assertEqual(owner.request("POST", "/v1/session", {}, auth=False,
                                              origin=ORIGIN, headers=headers)[0], 429)
                self.assertEqual(len(service.store.sessions._rates), 1)

    def test_relay_flag_does_not_enable_public_processing_or_change_owner_protocol(self):
        with running_service(expected_host=HOST, trust_relay_client_ip=True) as (service, owner):
            owner.headers = {"Host": HOST, RELAY_HEADER: "invalid"}
            self.assertEqual(owner.request("GET", "/v1/status", auth=False)[2],
                             {"api_version": 1, "status": "disabled", "guest_access": False})
            self.assertEqual(owner.request("GET", "/v1/capabilities")[0], 200)
            self.assertEqual(owner.request("GET", "/v1/capabilities", auth=False)[0], 401)
            self.assertEqual(owner.request("POST", "/v1/session", {}, auth=False, origin=ORIGIN)[0], 403)
            service.worker.process = lambda job: {}
            job = owner.create(b"x")[2]["id"]
            owner.upload(job, b"x")
            self.assertEqual(owner.request("POST", f"/v1/jobs/{job}/run", {})[0], 200)
            self.assertEqual(owner.request("DELETE", "/v1/jobs/" + job)[0], 200)
