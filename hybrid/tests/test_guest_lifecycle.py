"""Guest expiry/cancellation preserves worker, download and absolute read limits."""

import socket
import sys
import time
import unittest

from hybrid.tests.support import ORIGIN, mint_guest, process_alive, running_service, wait_until
from hybrid.tests import test_download_lifetime
from hybrid.tests.test_http_lifecycle import SLEEP_CHILD


class GuestLifecycleTests(unittest.TestCase):
    def queue(self, guest):
        job = guest.create(b"x")[2]["id"]
        guest.upload(job, b"x")
        self.assertEqual(guest.request("POST", f"/v1/jobs/{job}/run", {})[0], 200)
        return job

    def test_guest_expiry_reaps_native_child_and_shared_queue_remains_one_worker(self):
        with running_service(public_enabled=True, guest_session_ttl_seconds=1,
                             cleanup_interval_seconds=0.02) as (service, owner):
            service.worker.process = lambda job: service.worker.runner.run([sys.executable, "-c", SLEEP_CHILD], job, 40)
            guests = [mint_guest(owner)[0] for _ in range(2)]
            first = self.queue(guests[0])
            directory = service.store.jobs[first].directory
            wait_until(lambda: (directory / "native.pid").exists())
            pid = int((directory / "native.pid").read_text())
            second = self.queue(guests[1])
            second_directory = service.store.jobs[second].directory
            self.assertEqual(service.store.jobs[second].state, "queued")
            self.assertFalse((second_directory / "native.pid").exists())
            self.assertEqual(guests[1].request("DELETE", "/v1/jobs/" + first)[0], 404)
            wait_until(lambda: not directory.exists() and not second_directory.exists(), timeout=5)
            self.assertFalse(process_alive(pid))
            self.assertEqual(guests[0].request("GET", "/v1/jobs/" + first)[0], 401)

    def test_guest_slow_download_stops_at_session_expiry(self):
        with running_service(public_enabled=True, guest_session_ttl_seconds=1,
                             request_timeout_seconds=0.25, cleanup_interval_seconds=0.02) as (service, owner):
            guest, _ = mint_guest(owner)
            job, artifact = test_download_lifetime.DownloadLifetimeTests().completed_fixture(service, guest)
            connection = socket.socket()
            connection.settimeout(3)
            connection.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024)
            connection.connect(("127.0.0.1", guest.port))
            try:
                connection.sendall((f"GET /v1/jobs/{job.id}/artifacts/{artifact} HTTP/1.1\r\nHost: localhost\r\n"
                                    f"Authorization: Bearer {guest.token}\r\n\r\n").encode())
                wait_until(lambda: job.downloads > 0)
                wait_until(lambda: not job.directory.exists(), timeout=4)
                self.assertEqual(job.downloads, 0)
            finally:
                connection.close()

    def test_public_session_body_keeps_absolute_deadline_and_creates_no_session(self):
        with running_service(public_enabled=True, request_timeout_seconds=0.15) as (service, owner):
            connection = socket.create_connection(("127.0.0.1", owner.port), timeout=1)
            try:
                connection.sendall(("POST /v1/session HTTP/1.1\r\nHost: localhost\r\nOrigin: " + ORIGIN +
                                    "\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n").encode())
                started = time.monotonic()
                for _ in range(20):
                    try:
                        connection.sendall(b" ")
                    except OSError:
                        break
                    time.sleep(0.02)
                    if time.monotonic() - started > 0.25:
                        break
                wait_until(lambda: service.http.slots._value == service.config.max_connections, timeout=0.5)
                self.assertEqual(service.store.sessions._sessions, {})
                self.assertEqual(owner.request("GET", "/v1/status", auth=False)[0], 200)
            finally:
                connection.close()
