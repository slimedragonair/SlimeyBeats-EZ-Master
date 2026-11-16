"""An authenticated stalled reader cannot indefinitely keep expired audio alive."""

import socket
import time
import unittest

from hybrid.tests.support import TOKEN, running_service, wait_until


class DownloadLifetimeTests(unittest.TestCase):
    def completed_fixture(self, service, client):
        job_id = client.create(b"x")[2]["id"]
        job = service.store.jobs[job_id]
        out = job.directory / "out"
        out.mkdir()
        audio = out / "master.wav"
        with audio.open("wb") as stream:
            for _ in range(128):
                stream.write(b"\0" * 65536)
        artifact_id = "b" * 32
        with service.store.lock:
            job.state = "completed"
            job.artifacts[artifact_id] = {"id": artifact_id, "name": "master.wav", "bytes": audio.stat().st_size,
                                           "content_type": "audio/wav", "path": audio}
        return job, artifact_id

    def stalled_download(self, client, job_id, artifact_id):
        connection = socket.socket()
        connection.settimeout(3)
        connection.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024)
        connection.connect(("127.0.0.1", client.port))
        connection.sendall((f"GET /v1/jobs/{job_id}/artifacts/{artifact_id} HTTP/1.1\r\nHost: localhost\r\n"
                            f"Authorization: Bearer {TOKEN}\r\n\r\n").encode())
        return connection

    def test_delete_stops_slow_reader_and_releases_audio(self):
        with running_service(request_timeout_seconds=0.25) as (service, client):
            job, artifact = self.completed_fixture(service, client)
            connection = self.stalled_download(client, job.id, artifact)
            try:
                wait_until(lambda: job.downloads > 0)
                started = time.monotonic()
                self.assertEqual(client.request("DELETE", "/v1/jobs/" + job.id)[2], {"deleted": True})
                self.assertLess(time.monotonic() - started, 3)
                self.assertFalse(job.directory.exists())
                self.assertEqual(job.downloads, 0)
            finally:
                connection.close()

    def test_expiry_stops_slow_reader_and_reclaims_artifacts(self):
        with running_service(retention_seconds=1, request_timeout_seconds=0.25,
                             cleanup_interval_seconds=0.05) as (service, client):
            job, artifact = self.completed_fixture(service, client)
            connection = self.stalled_download(client, job.id, artifact)
            try:
                wait_until(lambda: job.downloads > 0)
                wait_until(lambda: not job.directory.exists(), timeout=4)
                self.assertEqual(job.downloads, 0)
            finally:
                connection.close()
