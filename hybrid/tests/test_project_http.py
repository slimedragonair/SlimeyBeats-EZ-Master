"""Authenticated v2 HTTP isolation, shared capacity, streaming uploads and leases."""

from concurrent.futures import ThreadPoolExecutor
import io
import json
from pathlib import Path
import secrets
import struct
import sys
import time
import unittest
from unittest.mock import patch

from hybrid.server.errors import ApiError
from hybrid.server.project_media import stream_media
from hybrid.tests.project_support import create_project, project_service
from hybrid.tests.support import mint_guest, ORIGIN


def seed_asset(service, project_id, payload=None):
    project = service.store.projects.projects[project_id]
    folder = project.directory / ("seed-" + secrets.token_hex(4))
    folder.mkdir()
    audio_id = secrets.token_hex(16)
    path = folder / "audio.wav"
    path.write_bytes(payload or b"audio-fixture")
    preview = folder / "preview.mp3"
    preview.write_bytes(payload or b"mp3-preview-fixture")
    waveform = folder / "waveform.json"
    waveform.write_text('{"duration_seconds":1,"peaks":[[-0.1,0.1]],"channel_peaks":[0.1]}')
    project.assets[audio_id] = {"id": audio_id, "role": "original", "name": "original", "path": path,
                               "preview_path": preview, "waveform_path": waveform, "bytes": path.stat().st_size,
                               "frames": 48000, "sample_rate": 48000, "channels": 1, "duration_seconds": 1,
                               "content_type": "audio/wav", "has_preview": True, "has_waveform": True}
    project.original_id = project.input_id = project.preview_id = audio_id
    return project, audio_id


class ProjectHttpTests(unittest.TestCase):
    def test_every_project_route_is_scoped_and_query_tokens_never_authenticate(self):
        with project_service(public_enabled=True) as (service, owner):
            first, session = mint_guest(owner)
            second, _ = mint_guest(owner)
            manifest = create_project(first)
            project, audio_id = seed_asset(service, manifest["id"])
            base = "/v2/projects/" + project.id
            upload = first.request("POST", base + "/uploads", {"filename": "x.wav", "bytes": 4, "role": "original"})[2]
            for method, route, options in (("GET", base, {}), ("DELETE", base, {}),
                                           ("POST", base + "/cancel", {"value": {}}),
                                           ("POST", base + "/commands", {"value": {"operation": "analyze", "base_revision": 0}}),
                                           ("POST", base + "/uploads", {"value": {"filename": "x.wav", "bytes": 4, "role": "original"}}),
                                           ("PUT", base + f"/uploads/{upload['id']}?offset=0", {"raw": b"abcd"}),
                                           ("GET", base + f"/assets/{audio_id}/audio", {}),
                                           ("GET", base + f"/assets/{audio_id}/preview", {}),
                                           ("GET", base + f"/assets/{audio_id}/waveform", {}),
                                           ("GET", base + "/artifacts/" + "a" * 32, {})):
                with self.subTest(route=route):
                    self.assertEqual(second.request(method, route, **options)[0], 404)
            self.assertEqual(first.request("GET", base, auth=False)[0], 401)
            self.assertEqual(first.request("GET", base + "?token=" + session["token"], auth=False)[0], 401)
            self.assertEqual(owner.request("GET", base)[0], 200)
            self.assertNotIn(session["token"], repr(service.store.projects.projects))

    def test_two_guest_projects_and_v1_share_four_retained_slots(self):
        with project_service(public_enabled=True) as (service, owner):
            guest, _ = mint_guest(owner)
            projects = [create_project(guest) for _ in range(2)]
            self.assertEqual(guest.request("POST", "/v2/projects", {})[0], 429)
            self.assertEqual(guest.create(b"x")[0], 429)
            self.assertEqual(owner.create(b"x")[0], 201)
            extra = create_project(owner)
            self.assertEqual(owner.request("GET", "/v1/status", auth=False)[2]["status"], "busy")
            self.assertEqual(owner.request("POST", "/v2/projects", {})[0], 429)
            self.assertEqual(guest.request("DELETE", "/v2/projects/" + projects[0]["id"])[2], {"deleted": True})
            self.assertEqual(owner.request("GET", "/v1/status", auth=False)[2]["status"], "online")
            self.assertEqual(len(service.store.retained()), 3)

    def test_range_cors_and_exact_streamed_append_with_partial_rollback(self):
        with project_service() as (service, client):
            manifest = create_project(client)
            base = "/v2/projects/" + manifest["id"]
            self.assertEqual(client.request("OPTIONS", base, auth=False, origin=ORIGIN,
                                           headers={"Access-Control-Request-Method": "GET", "Access-Control-Request-Headers": "authorization,range"})[0], 204)
            entry = client.request("POST", base + "/uploads", {"filename": "source.wav", "bytes": 8, "role": "original"})[2]
            route = base + "/uploads/" + entry["id"]
            self.assertEqual(client.request("PUT", route + "?offset=0", raw=b"abcd")[2]["uploaded_bytes"], 4)
            self.assertEqual(client.request("PUT", route + "?offset=0", raw=b"abcd")[0], 409)
            self.assertEqual(client.request("PUT", route + "?offset=4&offset=4", raw=b"abcd")[0], 400)
            with self.assertRaises(ApiError):
                service.store.projects.upload(manifest["id"], entry["id"], 4, 4, io.BytesIO(b"x"))
            stored = service.store.projects.projects[manifest["id"]].uploads[entry["id"]]
            self.assertEqual((stored.uploaded_bytes, stored.path.read_bytes()), (4, b"abcd"))
            with ThreadPoolExecutor(2) as executor:
                results = list(executor.map(lambda _: client.request("PUT", route + "?offset=4", raw=b"efgh")[0], range(2)))
            self.assertEqual(sorted(results), [200, 409])
            project, audio_id = seed_asset(service, manifest["id"])
            media = base + f"/assets/{audio_id}/preview"
            status, headers, payload = client.request("GET", media, origin=ORIGIN, headers={"Range": "bytes=2-5"})
            self.assertEqual((status, payload), (206, b"3-pr"))
            self.assertEqual(headers["Content-Range"], "bytes 2-5/19")
            self.assertIn("Content-Range", headers["Access-Control-Expose-Headers"])
            status, headers, _ = client.request("GET", media, headers={"Range": "bytes=-0"})
            self.assertEqual((status, headers["Content-Range"]), (416, "bytes */19"))

    def test_streaming_reads_bounded_blocks_and_releases_lease(self):
        with project_service() as (service, client):
            manifest = create_project(client)
            project, audio_id = seed_asset(service, manifest["id"], b"x" * (1024 * 1024 + 1))
            class Headers:
                def get_all(self, name, default=None):
                    return []
            class Sink:
                count = 0
                largest = 0
                def write(self, block):
                    self.count += len(block)
                    self.largest = max(self.largest, len(block))
            handler = type("Handler", (), {"headers": Headers(), "wfile": Sink(), "headers_for": lambda *a, **kw: None})()
            stream_media(handler, service.store.projects, project.id, audio_id, "audio", __import__("hybrid.server.principals", fromlist=["OWNER"]).OWNER)
            self.assertEqual(handler.wfile.count, 1024 * 1024 + 1)
            self.assertLessEqual(handler.wfile.largest, 65536)
            self.assertTrue(project.downloads_done.is_set())

    def test_retention_clamps_to_guest_and_expiry_removes_both_projects(self):
        with project_service(public_enabled=True, guest_session_ttl_seconds=1) as (service, owner):
            guest, _ = mint_guest(owner)
            projects = [create_project(guest) for _ in range(2)]
            directories = [service.store.projects.projects[entry["id"]].directory for entry in projects]
            for project in service.store.projects.projects.values():
                self.assertLessEqual(project.deadline, time.monotonic() + 1)
            from hybrid.tests.support import wait_until
            wait_until(lambda: all(not folder.exists() for folder in directories), timeout=5)
            self.assertEqual(guest.request("GET", "/v2/capabilities")[0], 401)
