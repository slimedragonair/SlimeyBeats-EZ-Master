"""Queue/cancel/expiry tests use real supervised children, not a mastering substitute."""

import sys
import time
import unittest
from unittest.mock import patch

from hybrid.server.errors import ProcessingError
from hybrid.server.errors import ApiError
from hybrid.tests.support import process_alive, running_service, wait_until

SLEEP_CHILD = "import os,pathlib,time; pathlib.Path('native.pid').write_text(str(os.getpid())); time.sleep(30)"


class HttpLifecycleTests(unittest.TestCase):
    def install_sleep_worker(self, service):
        def controlled_process(job):
            service.worker.runner.run([sys.executable, "-c", SLEEP_CHILD], job, 40)
            raise ProcessingError("The control fixture must be cancelled")
        service.worker.process = controlled_process

    def queue(self, client):
        job = client.create(b"x")[2]["id"]
        client.upload(job, b"x")
        self.assertEqual(client.request("POST", f"/v1/jobs/{job}/run", {})[0], 200)
        return job

    def test_one_worker_idempotent_run_and_safe_running_cancel(self):
        with running_service() as (service, client):
            self.install_sleep_worker(service)
            first = self.queue(client)
            directory = service.store.jobs[first].directory
            wait_until(lambda: (directory / "native.pid").exists())
            pid = int((directory / "native.pid").read_text())
            self.assertTrue(process_alive(pid))
            second = self.queue(client)
            self.assertEqual(client.request("GET", "/v1/jobs/" + second)[2]["state"], "queued")
            for _ in range(3):
                self.assertEqual(client.request("POST", f"/v1/jobs/{first}/run", {})[2]["state"], "running")
                self.assertEqual(client.request("POST", f"/v1/jobs/{second}/run", {})[2]["state"], "queued")
            self.assertEqual(list(service.store.queue), [second])
            self.assertEqual(client.request("PUT", f"/v1/jobs/{first}/upload?offset=1", raw=b"x")[0], 409)
            self.assertEqual(client.request("DELETE", "/v1/jobs/" + second)[2], {"deleted": True})
            self.assertFalse((service.storage.root / ("job-" + second)).exists())
            self.assertEqual(client.request("DELETE", "/v1/jobs/" + first)[2], {"deleted": True})
            self.assertFalse(process_alive(pid), "DELETE returned before its child stopped")
            self.assertFalse(directory.exists())
            self.assertEqual(client.request("GET", "/v1/jobs/" + first)[0], 404)

    def test_expiry_removes_upload_and_running_process(self):
        with running_service(retention_seconds=1, cleanup_interval_seconds=0.05) as (service, client):
            self.install_sleep_worker(service)
            partial = client.create(b"abcd")[2]["id"]
            client.request("PUT", f"/v1/jobs/{partial}/upload?offset=0", raw=b"ab")
            running = self.queue(client)
            directory = service.store.jobs[running].directory
            wait_until(lambda: (directory / "native.pid").exists())
            pid = int((directory / "native.pid").read_text())
            wait_until(lambda: not (service.storage.root / ("job-" + partial)).exists(), timeout=5)
            wait_until(lambda: not directory.exists(), timeout=5)
            self.assertFalse(process_alive(pid))
            self.assertEqual(client.request("GET", "/v1/jobs/" + running)[0], 404)

    def test_failure_message_and_logs_never_contain_child_stderr(self):
        with running_service() as (service, client):
            private = "C:/private-owner/secret-token.wav"
            def failing(job):
                code = "import sys; sys.stderr.write(" + repr(private) + " * 10000); sys.exit(7)"
                service.worker.runner.run([sys.executable, "-c", code], job, 5)
            service.worker.process = failing
            job = self.queue(client)
            snapshot = client.wait(job)
            self.assertEqual(snapshot["state"], "failed")
            self.assertNotIn("private-owner", str(snapshot))
            self.assertNotIn("secret-token", str(snapshot))
            self.assertEqual(snapshot["artifacts"], [])
            self.assertEqual([item.name for item in service.store.jobs[job].directory.iterdir()], [".slimeybeats-hybrid-job"])

    def test_one_cleanup_failure_does_not_skip_other_expired_jobs(self):
        with running_service(cleanup_interval_seconds=10) as (service, client):
            jobs = [client.create(b"x")[2]["id"] for _ in range(2)]
            for job_id in jobs:
                service.store.jobs[job_id].deadline = time.monotonic() - 1
            original = service.store.delete

            def fail_first(job_id, expired=False):
                if job_id == jobs[0]:
                    raise ApiError(503, "Cleanup needs retry")
                return original(job_id, expired=expired)

            with patch.object(service.store, "delete", side_effect=fail_first):
                with self.assertRaises(ApiError):
                    service.store.expire()
            self.assertIn(jobs[0], service.store.jobs)
            self.assertNotIn(jobs[1], service.store.jobs)


if __name__ == "__main__":
    unittest.main()
