"""Single shared heavy worker, transactional cancellation and retained quotas."""

from pathlib import Path
import sys
import time
import unittest
from unittest.mock import patch

from hybrid.server.errors import ApiError, ProcessingError
from hybrid.tests.project_support import command, create_project, project_service
from hybrid.tests.test_project_http import seed_asset
from hybrid.tests.support import mint_guest, process_alive, wait_until

SLEEP = "import os,pathlib,time; pathlib.Path('native.pid').write_text(str(os.getpid())); time.sleep(30)"


class ProjectLifecycleTests(unittest.TestCase):
    def test_v1_and_v2_use_exactly_one_fifo_worker(self):
        with project_service(ffmpeg=Path(sys.executable)) as (service, client):
            def controlled(job):
                service.worker.runner.run([sys.executable, "-c", SLEEP], job, 40)
                raise ProcessingError("Control must be cancelled")
            service.worker.process = controlled
            called = []
            service.worker.project_worker.process = lambda item: (called.append(item.id) or ({}, {}, None, {}))
            v1 = client.create(b"x")[2]["id"]
            client.upload(v1, b"x")
            client.request("POST", f"/v1/jobs/{v1}/run", {})
            wait_until(lambda: (service.store.jobs[v1].directory / "native.pid").exists())
            manifest = create_project(client)
            seed_asset(service, manifest["id"])
            base = "/v2/projects/" + manifest["id"]
            queued = client.request("POST", base + "/commands", {"operation": "analyze", "base_revision": 0})[2]
            self.assertEqual(queued["command"]["state"], "queued")
            time.sleep(0.1)
            self.assertEqual(called, [])
            self.assertEqual(list(service.store.queue), [(manifest["id"], queued["command"]["id"])])
            self.assertEqual(client.request("DELETE", "/v1/jobs/" + v1)[2], {"deleted": True})
            wait_until(lambda: len(called) == 1)
            wait_until(lambda: service.store.projects.projects[manifest["id"]].command.state == "completed")
            self.assertEqual(service.store.projects.projects[manifest["id"]].revision, 1)

    def test_running_cancel_reaps_child_removes_staging_and_keeps_committed_audio(self):
        with project_service(ffmpeg=Path(sys.executable)) as (service, client):
            manifest = create_project(client)
            project, original_id = seed_asset(service, manifest["id"])
            path = project.assets[original_id]["path"]
            def controlled(item):
                item.stage.mkdir()
                (item.stage / "partial.wav").write_bytes(b"partial")
                service.worker.project_worker.runner.run([sys.executable, "-c", SLEEP], item, 40)
                raise ProcessingError("Cancelled control")
            service.worker.project_worker.process = controlled
            base = "/v2/projects/" + project.id
            self.assertEqual(client.request("POST", base + "/commands", {"operation": "analyze", "base_revision": 0})[0], 202)
            pidfile = project.directory / "native.pid"
            wait_until(pidfile.exists)
            pid = int(pidfile.read_text())
            result = client.request("POST", base + "/cancel", {})
            self.assertEqual(result[0], 200)
            self.assertEqual(result[2]["command"]["state"], "cancelled")
            self.assertEqual((result[2]["revision"], result[2]["original_id"]), (0, original_id))
            self.assertFalse(process_alive(pid))
            self.assertFalse(project.command.stage.exists())
            self.assertEqual(path.read_bytes(), b"audio-fixture")
            self.assertEqual(client.request("GET", base + f"/assets/{original_id}/audio")[0], 200)

    def test_stale_revision_clone_isolation_and_metadata_history(self):
        with project_service(public_enabled=True, ffmpeg=Path(sys.executable)) as (service, owner):
            first, _ = mint_guest(owner)
            second, _ = mint_guest(owner)
            main = create_project(first)
            destination = create_project(second)
            project, audio_id = seed_asset(service, main["id"])
            route = "/v2/projects/" + destination["id"] + "/commands"
            self.assertEqual(second.request("POST", route, {"operation": "clone", "base_revision": 0,
                                                            "source_project_id": main["id"], "audio_id": audio_id})[0], 404)
            same_owner = owner.request("POST", route, {"operation": "clone", "base_revision": 0,
                                                        "source_project_id": main["id"], "audio_id": audio_id})[0]
            self.assertEqual(same_owner, 404)
            base = "/v2/projects/" + main["id"]
            promoted = command(first, main, "promote", audio_id=audio_id)
            self.assertEqual(promoted["revision"], 1)
            self.assertEqual(first.request("POST", base + "/commands", {"operation": "promote", "base_revision": 0, "audio_id": audio_id})[0], 409)
            undo = command(first, promoted, "history", redo=False)
            self.assertEqual((undo["revision"], undo["can_redo"]), (2, True))
            redo = command(first, undo, "history", redo=True)
            self.assertEqual(redo["revision"], 3)

    def test_v2_quota_allows_120_edits_and_does_not_spend_v1_starts(self):
        with project_service(public_enabled=True, ffmpeg=Path(sys.executable), guest_jobs_per_session=3) as (service, owner):
            guest, _ = mint_guest(owner)
            principal = service.store.sessions.authenticate(guest.token)
            key = service.store.sessions.client_key("127.0.0.1")
            for _ in range(120):
                service.store.sessions.start_command(principal, key)
            with self.assertRaises(ApiError) as caught:
                service.store.sessions.start_command(principal, key)
            self.assertEqual(caught.exception.status, 429)
            for _ in range(3):
                service.store.sessions.start_job(principal, key)
            with self.assertRaises(ApiError):
                service.store.sessions.start_job(principal, key)
            self.assertEqual(len(service.store.sessions._totals), 3)

    def test_cleanup_failure_is_retryable(self):
        with project_service() as (service, client):
            manifest = create_project(client)
            project, audio_id = seed_asset(service, manifest["id"])
            with patch.object(service.storage, "remove", side_effect=OSError("private path")):
                result = client.request("DELETE", "/v2/projects/" + project.id)
            self.assertEqual(result[0], 503)
            self.assertEqual(result[2], {"error": "cleanup_failed"})
            self.assertTrue(project.directory.exists())
            self.assertEqual(client.request("DELETE", "/v2/projects/" + project.id)[2], {"deleted": True})
            self.assertFalse(project.directory.exists())
