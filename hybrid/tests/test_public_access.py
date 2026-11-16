"""Public discovery, session boundaries and principal-scoped HTTP operations."""

from concurrent.futures import ThreadPoolExecutor
from datetime import datetime
import io
import json
import logging
import time
import unittest

from hybrid.tests.support import ORIGIN, TOKEN, mint_guest, running_service, wait_until


class PublicAccessTests(unittest.TestCase):
    def test_disabled_status_is_exact_safe_and_owner_still_works(self):
        with running_service() as (service, owner):
            status, headers, value = owner.request("GET", "/v1/status", auth=False, origin=ORIGIN,
                                                 headers={"Authorization": "invalid"})
            self.assertEqual((status, value), (200, {"api_version": 1, "status": "disabled", "guest_access": False}))
            self.assertEqual(headers["Access-Control-Allow-Origin"], ORIGIN)
            self.assertEqual(headers["Cache-Control"], "no-store")
            self.assertNotIn("Access-Control-Allow-Credentials", headers)
            self.assertEqual(owner.request("GET", "/v1/status", auth=False)[0], 200)
            for method, path in (("POST", "/v1/status"), ("HEAD", "/v1/status"),
                                 ("GET", "/v1/status/"), ("GET", "/v1/status?token=secret"),
                                 ("GET", "/v1/status?"), ("GET", "/v1/session")):
                self.assertEqual(owner.request(method, path, auth=False)[0], 401)
            self.assertEqual(owner.request("POST", "/v1/session", {}, auth=False, origin=ORIGIN)[0], 403)
            job = owner.create(b"x")[2]["id"]
            self.assertEqual(owner.request("GET", "/v1/jobs/" + job)[0], 200)
            self.assertEqual(len(service.store.jobs), 1)

    def test_session_origin_schema_expiry_and_token_privacy(self):
        stream = io.StringIO()
        logger = logging.getLogger("slimeybeats.hybrid")
        capture = logging.StreamHandler(stream)
        logger.addHandler(capture)
        try:
            with running_service(public_enabled=True) as (service, owner):
                for origin in (None, "null", "https://evil.example"):
                    self.assertEqual(owner.request("POST", "/v1/session", {}, auth=False, origin=origin)[0], 403)
                for value in ([], {"token": TOKEN}, {"id": "a" * 32}):
                    self.assertEqual(owner.request("POST", "/v1/session", value, auth=False, origin=ORIGIN)[0], 400)
                guest, session = mint_guest(owner)
                self.assertEqual(set(session), {"token", "expires_at"})
                self.assertRegex(session["token"], r"^[A-Za-z0-9_-]{43}$")
                expiry = datetime.fromisoformat(session["expires_at"].replace("Z", "+00:00")).timestamp()
                self.assertGreater(expiry, time.time())
                self.assertLessEqual(expiry, time.time() + 3600.001)  # ISO timestamps round to microseconds.
                self.assertEqual(guest.request("GET", "/v1/capabilities")[0], 200)
                self.assertEqual(owner.request("GET", "/v1/capabilities?token=" + guest.token, auth=False)[0], 401)
                self.assertNotIn(guest.token, repr(service.store.sessions.__dict__))
                self.assertNotIn("127.0.0.1", repr((service.store.sessions._sessions,
                                                   service.store.sessions._rates)))
                self.assertNotIn(TOKEN, json.dumps(session))
                self.assertEqual(owner.request("OPTIONS", "/v1/session", auth=False, origin=ORIGIN,
                                              headers={"Access-Control-Request-Method": "POST",
                                                       "Access-Control-Request-Headers": "content-type"})[0], 204)
                self.assertEqual(owner.request("GET", "//v1/status", auth=False)[0], 400)
            self.assertNotIn(guest.token, stream.getvalue())
            self.assertNotIn(TOKEN, stream.getvalue())
        finally:
            logger.removeHandler(capture)

    def test_guests_cannot_read_upload_run_cancel_or_download_each_others_jobs(self):
        with running_service(public_enabled=True) as (service, owner):
            first, _ = mint_guest(owner)
            second, _ = mint_guest(owner)
            jobs = [guest.create(b"x")[2]["id"] for guest in (first, second)]
            owner_job = owner.create(b"x")[2]["id"]
            for guest, own, foreign in ((first, jobs[0], jobs[1]), (second, jobs[1], jobs[0])):
                self.assertEqual(guest.request("GET", "/v1/jobs/" + own)[0], 200)
                for job in (foreign, owner_job, "f" * 32):
                    route = "/v1/jobs/" + job
                    for method, path, options in (("GET", route, {}), ("DELETE", route, {}),
                        ("PUT", route + "/upload?offset=0", {"raw": b"x"}),
                        ("POST", route + "/run", {"value": {}}),
                        ("GET", route + "/artifacts/" + "b" * 32, {})):
                        self.assertEqual(guest.request(method, path, **options)[0:3:2],
                                         (404, {"error": "Job not found or expired"}))
                self.assertEqual(guest.request("GET", "/v1/jobs")[0], 404)
                self.assertEqual(guest.request("GET", "/v1/status")[2],
                                 {"api_version": 1, "status": "online", "guest_access": True})
            # Known IDs never assign ownership; all three scopes remain separate.
            for job in jobs + [owner_job]:
                self.assertEqual(owner.request("GET", "/v1/jobs/" + job)[0], 200)
                owner.upload(job, b"x")
                self.assertEqual(owner.request("DELETE", "/v1/jobs/" + job)[0], 200)
            self.assertEqual(service.store.jobs, {})

    def test_idempotency_is_namespaced_and_session_has_one_retained_job(self):
        with running_service(public_enabled=True) as (service, owner):
            guests = [mint_guest(owner)[0] for _ in range(2)]
            value = {"filename": "x.wav", "bytes": 1, "consent": True}
            headers = {"Idempotency-Key": "shared_retry_0123456789"}
            ids = []
            for client in [owner, *guests]:
                first = client.request("POST", "/v1/jobs", value, headers=headers)
                self.assertEqual(first[0], 201)
                self.assertEqual(client.request("POST", "/v1/jobs", value, headers=headers)[2], first[2])
                self.assertEqual(client.request("POST", "/v1/jobs", {**value, "bytes": 2}, headers=headers)[0], 409)
                ids.append(first[2]["id"])
            self.assertEqual(len(set(ids)), 3)
            for client in guests:
                limited = client.create(b"x")
                self.assertEqual(limited[0], 429)
                self.assertGreater(int(limited[1]["Retry-After"]), 0)
            self.assertEqual(len(service.store.idempotency), 3)
            for index in range(50):
                rotated = {"Idempotency-Key": f"rotated_key_{index:016d}"}
                self.assertEqual(guests[0].request("POST", "/v1/jobs", value, headers=rotated)[0], 429)
            self.assertEqual(len(service.store.idempotency), 3)
            owner.request("DELETE", "/v1/jobs/" + ids[1])
            self.assertEqual(guests[0].create(b"x")[0], 201)

    def test_global_capacity_status_and_concurrent_guest_creation(self):
        with running_service(public_enabled=True) as (service, owner):
            guest, _ = mint_guest(owner)
            with ThreadPoolExecutor(max_workers=8) as pool:
                responses = list(pool.map(lambda _: guest.create(b"x")[0], range(8)))
            self.assertEqual(responses.count(201), 1)
            self.assertEqual(responses.count(429), 7)
            jobs = [owner.create(b"x")[2]["id"] for _ in range(3)]
            status = owner.request("GET", "/v1/status", auth=False)[2]
            self.assertEqual(status, {"api_version": 1, "status": "busy", "guest_access": True})
            self.assertEqual(len(service.store.jobs), 4)
            limited = owner.create(b"x")
            self.assertEqual(limited[0], 429)
            self.assertIn("Retry-After", limited[1])
            owner.request("DELETE", "/v1/jobs/" + jobs[0])
            self.assertEqual(owner.request("GET", "/v1/status", auth=False)[2]["status"], "online")

    def test_expired_bearer_is_401_and_job_expiry_is_clamped_and_purged(self):
        with running_service(public_enabled=True, guest_session_ttl_seconds=1,
                             retention_seconds=3600, cleanup_interval_seconds=0.02) as (service, owner):
            guest, session = mint_guest(owner)
            created = guest.create(b"xy")[2]
            job = service.store.jobs[created["id"]]
            principal = service.store.sessions.authenticate(guest.token)
            self.assertEqual(created["expires_at"], session["expires_at"])
            self.assertEqual(job.deadline, principal.deadline)
            guest.upload(job.id, b"xy")
            wait_until(lambda: not job.directory.exists(), timeout=4)
            for method, path, options in (("GET", "/v1/capabilities", {}),
                ("GET", "/v1/jobs/" + job.id, {}), ("DELETE", "/v1/jobs/" + job.id, {}),
                ("POST", "/v1/jobs/" + job.id + "/run", {"value": {}}),
                ("PUT", "/v1/jobs/" + job.id + "/upload?offset=0", {"raw": b"x"}),
                ("GET", "/v1/jobs/" + job.id + "/artifacts/" + "b" * 32, {})):
                self.assertEqual(guest.request(method, path, **options)[0], 401)
            self.assertEqual(service.store.sessions._sessions, {})
            self.assertEqual(owner.request("GET", "/v1/capabilities")[0], 200)

    def test_session_flood_is_429_with_exposed_retry_after(self):
        with running_service(public_enabled=True) as (service, owner):
            for _ in range(4):
                mint_guest(owner)
            for _ in range(50):
                status, headers, _ = owner.request("POST", "/v1/session", {}, auth=False, origin=ORIGIN,
                                                  headers={"X-Forwarded-For": "203.0.113.2"})
                self.assertEqual(status, 429)
                self.assertGreater(int(headers["Retry-After"]), 0)
                self.assertIn("Retry-After", headers["Access-Control-Expose-Headers"])
            self.assertEqual(len(service.store.sessions._sessions), 4)
            self.assertEqual(len(service.store.sessions._rates), 1)
