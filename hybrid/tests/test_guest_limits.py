"""Rate enforcement and registry memory bounds under key/address rotation."""

from concurrent.futures import ThreadPoolExecutor
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from hybrid.server.errors import ApiError
from hybrid.server.guest_sessions import GuestRegistry, MAX_RATE_ENTRIES, MAX_SESSIONS
from hybrid.tests.support import mint_guest, running_service


class GuestLimitTests(unittest.TestCase):
    def registry(self, ttl=3600):
        clock = [0.0]
        registry = GuestRegistry(SimpleNamespace(public_enabled=True, guest_session_ttl_seconds=ttl,
                                 guest_jobs_per_session=3, guest_jobs_per_ip_hour=8, guest_jobs_per_hour=32),
                                 monotonic=lambda: clock[0], wall_time=lambda: 1700000000 + clock[0])
        return registry, clock

    def limited(self, action):
        with self.assertRaises(ApiError) as caught:
            action()
        self.assertEqual(caught.exception.status, 429)
        self.assertGreater(caught.exception.retry_after, 0)
        return caught.exception.retry_after

    def test_session_global_cap_concurrency_and_invalid_key_flood(self):
        registry, clock = self.registry()
        def attempt(index):
            try:
                return registry.mint(registry.client_key("203.0.113." + str(index)))
            except ApiError as error:
                self.assertEqual(error.status, 429)
                return None
        with ThreadPoolExecutor(max_workers=8) as pool:
            responses = list(pool.map(attempt, range(128)))
        self.assertEqual(sum(value is not None for value in responses), MAX_SESSIONS)
        self.assertEqual(len(registry._sessions), MAX_SESSIONS)
        clock[0] = 601
        self.assertGreater(self.limited(lambda: registry.mint(registry.client_key("198.51.100.1"))), 2900)
        for index in range(1000):
            with self.assertRaises(ApiError) as caught:
                registry.authenticate(f"{index:043d}")
            self.assertEqual(caught.exception.status, 401)
        self.assertEqual(len(registry._sessions), MAX_SESSIONS)
        self.assertLessEqual(len(registry._rates), MAX_RATE_ENTRIES)
        clock[0] = 3601
        registry.expire()
        self.assertEqual((registry._sessions, registry._rates, registry._totals), ({}, {}, {}))
        self.assertIn("token", registry.mint(registry.client_key("198.51.100.1")))

    def test_expiry_does_not_reset_watched_windows_and_nonce_is_process_private(self):
        registry, clock = self.registry(ttl=1)
        key = registry.client_key("203.0.113.1")
        other, _ = self.registry()
        self.assertNotEqual(key, other.client_key("203.0.113.1"))
        for _ in range(4):
            registry.mint(key)
            clock[0] += 2
            registry.expire()
        self.assertEqual(registry._sessions, {})
        self.assertEqual(self.limited(lambda: registry.mint(key)), 592)
        for index in range(60):
            registry.mint(registry.client_key(f"198.51.100.{index}"))
            clock[0] += 2
        self.limited(lambda: registry.mint(registry.client_key("192.0.2.1")))
        self.assertEqual(len(registry._rates), 61)
        clock[0] = 601
        registry.mint(key)
        self.assertLessEqual(len(registry._rates), MAX_RATE_ENTRIES)

    def test_live_rate_entries_are_never_evicted_to_accept_rotated_addresses(self):
        registry, clock = self.registry(ttl=1)
        with patch("hybrid.server.guest_sessions.MAX_RATE_ENTRIES", 4):
            for index in range(4):
                registry.mint(registry.client_key(f"203.0.113.{index}"))
                clock[0] += 2
            original = dict(registry._rates)
            for index in range(100):
                self.limited(lambda: registry.mint(registry.client_key(f"198.51.100.{index}")))
            self.assertEqual(registry._rates, original)
            clock[0] = 607
            registry.mint(registry.client_key("192.0.2.1"))
            self.assertEqual(len(registry._rates), 1)

    def test_http_run_limits_per_guest_and_peer_and_retries_are_idempotent(self):
        with running_service(public_enabled=True, guest_jobs_per_session=3,
                             guest_jobs_per_ip_hour=8, guest_jobs_per_hour=32) as (service, owner):
            service.worker.process = lambda job: {}
            guests = [mint_guest(owner)[0] for _ in range(3)]
            for guest_index, guest in enumerate(guests):
                for index in range(3):
                    job = guest.create(b"x")[2]["id"]
                    guest.upload(job, b"x")
                    result = guest.request("POST", f"/v1/jobs/{job}/run", {})
                    if guest_index == 2 and index == 2:
                        self.assertEqual(result[0], 429)
                        self.assertGreater(int(result[1]["Retry-After"]), 0)
                    else:
                        self.assertEqual(result[0], 200)
                        for _ in range(5):
                            self.assertEqual(guest.request("POST", f"/v1/jobs/{job}/run", {})[0], 200)
                    guest.request("DELETE", "/v1/jobs/" + job)
                if guest_index == 0:
                    job = guest.create(b"x")[2]["id"]
                    guest.upload(job, b"x")
                    result = guest.request("POST", f"/v1/jobs/{job}/run", {})
                    self.assertEqual(result[0], 429)
                    self.assertGreater(int(result[1]["Retry-After"]), 0)
                    guest.request("DELETE", "/v1/jobs/" + job)
            # Owner starts remain usable even when the public IP budget is exhausted.
            job = owner.create(b"x")[2]["id"]
            owner.upload(job, b"x")
            self.assertEqual(owner.request("POST", f"/v1/jobs/{job}/run", {})[0], 200)
            self.assertLessEqual(len(service.store.sessions._rates), MAX_RATE_ENTRIES)

    def test_global_job_start_limit_survives_address_and_guest_rotation(self):
        registry, clock = self.registry(ttl=1)
        for index in range(32):
            key = registry.client_key(f"198.51.100.{index}")
            token = registry.mint(key)["token"]
            registry.start_job(registry.authenticate(token), key)
            clock[0] += 2
        key = registry.client_key("192.0.2.1")
        token = registry.mint(key)["token"]
        self.limited(lambda: registry.start_job(registry.authenticate(token), key))
        self.assertLessEqual(len(registry._rates), MAX_RATE_ENTRIES)
        self.assertEqual(len(registry._totals), 2)
