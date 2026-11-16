"""Proxy headers are trusted only at the operator-selected loopback boundary."""

from email.message import Message
from pathlib import Path
import os
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from hybrid.server.client_identity import client_key
from hybrid.server.config import Config, from_arguments
from hybrid.server.errors import ApiError
from hybrid.tests.support import ORIGIN, TOKEN, running_service

HOST = "api.slimedragon.net"


class ClientIdentityTests(unittest.TestCase):
    def test_public_environment_is_explicit_and_proxy_configuration_fails_closed(self):
        for value in ("", "0", "true", "yes", "1 ", "1"):
            with patch.dict(os.environ, {"SLIMEYBEATS_HYBRID_TOKEN": TOKEN, "SLIMEYBEATS_HYBRID_PUBLIC": value}):
                config = from_arguments(["--cli", sys.executable])
                self.assertEqual(config.public_enabled, value == "1")
        defaults = dict(cli=Path(sys.executable), token=TOKEN)
        for extra in ({"trust_cloudflare_client_ip": True},
                      {"trust_cloudflare_client_ip": True, "expected_host": "evil.example"},
                      {"trust_cloudflare_client_ip": True, "expected_host": HOST, "host": "0.0.0.0"},
                      {"guest_session_ttl_seconds": 3601}, {"guest_session_ttl_seconds": 0}):
            with self.assertRaises(ValueError):
                Config(**defaults, **extra)
        with patch.dict(os.environ, {"SLIMEYBEATS_HYBRID_TOKEN": TOKEN}):
            config = from_arguments(["--cli", sys.executable, "--trust-cloudflare-client-ip", "--expected-host", HOST])
        self.assertTrue(config.trust_cloudflare_client_ip)
        self.assertEqual(config.expected_host, HOST)

    def test_untrusted_cf_and_forwarded_headers_cannot_rotate_peer_identity(self):
        with running_service(public_enabled=True) as (service, owner):
            for index in range(4):
                result = owner.request("POST", "/v1/session", {}, auth=False, origin=ORIGIN,
                                       headers={"CF-Connecting-IP": f"203.0.113.{index}",
                                                "X-Forwarded-For": f"198.51.100.{index}"})
                self.assertEqual(result[0], 201)
            result = owner.request("POST", "/v1/session", {}, auth=False, origin=ORIGIN,
                                   headers={"CF-Connecting-IP": "192.0.2.1", "X-Forwarded-For": "192.0.2.2"})
            self.assertEqual(result[0], 429)
            self.assertEqual(len(service.store.sessions._rates), 1)

    def test_trusted_loopback_requires_exact_host_and_single_valid_cf_address(self):
        with running_service(public_enabled=True, expected_host=HOST,
                             trust_cloudflare_client_ip=True) as (service, owner):
            for headers in ({}, {"Host": "evil.example", "CF-Connecting-IP": "203.0.113.1"},
                            {"Host": HOST}, {"Host": HOST, "CF-Connecting-IP": "bad"},
                            {"Host": HOST, "CF-Connecting-IP": "203.0.113.1, 203.0.113.2"},
                            {"Host": HOST, "CF-Connecting-IP": "fe80::1%ethernet"}):
                self.assertEqual(owner.request("POST", "/v1/session", {}, auth=False,
                                              origin=ORIGIN, headers=headers)[0], 400)
            for index in range(8):
                result = owner.request("POST", "/v1/session", {}, auth=False, origin=ORIGIN,
                                       headers={"Host": HOST, "CF-Connecting-IP": f"203.0.113.{index}"})
                self.assertEqual(result[0], 201)
            self.assertEqual(len(service.store.sessions._rates), 8)
            self.assertNotIn("203.0.113", repr(service.store.sessions._rates))
            headers = Message()
            headers["Host"] = HOST
            headers["CF-Connecting-IP"] = "203.0.113.1"
            handler = SimpleNamespace(headers=headers, server=service.http, client_address=("198.51.100.1", 123))
            with self.assertRaises(ApiError):
                client_key(handler)
            handler.client_address = ("127.0.0.1", 123)
            headers["CF-Connecting-IP"] = "203.0.113.2"
            with self.assertRaises(ApiError):
                client_key(handler)
            self.assertEqual(owner.request("GET", "/v1/status", auth=False,
                                          headers={"Host": HOST})[0], 200)
