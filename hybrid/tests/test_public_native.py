"""Minted browser sessions through the actual CLI, exports and source privacy."""

import io
import json
import struct
import unittest
import wave

from hybrid.tests.support import TOKEN, mint_guest, running_service
from hybrid.tests.test_audio_guard import custom_wav
from hybrid.tests.test_native_cli import native_tools


class PublicNativeTests(unittest.TestCase):
    def test_two_minted_guests_native_exports_are_scoped_and_source_private(self):
        cli, ffmpeg = native_tools(self)
        with running_service(public_enabled=True, cli=cli, ffmpeg=ffmpeg,
                             max_job_disk_bytes=128 * 1024 * 1024) as (service, owner):
            guests = [mint_guest(owner)[0] for _ in range(2)]
            private = b"private-source-host/path/to/unreleased-song"
            private += b"\0" * (len(private) % 2)
            metadata = b"JUNK" + struct.pack("<I", len(private)) + private
            data = custom_wav(encoding=3, bits=32, data=struct.pack("<f", 0.25) * 48000, metadata=metadata)
            jobs = []
            for index, guest in enumerate(guests):
                fmt = "mp3" if index and ffmpeg else "wav24"
                status, _, created = guest.create(data, filename="unreleased-secret-song.wav", format=fmt)
                self.assertEqual(status, 201)
                job = created["id"]
                jobs.append(job)
                guest.upload(job, data)
                self.assertEqual(guest.request("POST", f"/v1/jobs/{job}/run", {})[0], 200)
                result = guest.wait(job)
                self.assertEqual(result["state"], "completed", result)
                self.assertEqual(len(result["artifacts"]), 2)
                other = guests[1 - index]
                self.assertEqual(other.request("GET", "/v1/jobs/" + job)[0], 404)
                self.assertEqual(other.request("DELETE", "/v1/jobs/" + job)[0], 404)
                for artifact in result["artifacts"]:
                    route = f"/v1/jobs/{job}/artifacts/{artifact['id']}"
                    self.assertEqual(other.request("GET", route)[0], 404)
                    status, headers, content = guest.request("GET", route)
                    self.assertEqual(status, 200)
                    self.assertEqual(owner.request("GET", route)[2], content)
                    raw = content if isinstance(content, bytes) else json.dumps(content).encode()
                    for secret in (private.rstrip(b"\0"), b"unreleased-secret-song", TOKEN.encode(),
                                   guests[0].token.encode(), guests[1].token.encode(),
                                   str(service.storage.root).encode()):
                        self.assertNotIn(secret, raw)
                    if artifact["name"] == "master.wav":
                        with wave.open(io.BytesIO(content)) as audio:
                            self.assertEqual(audio.getsampwidth(), 3)
                            self.assertEqual(audio.getnframes(), 48000)
                    self.assertEqual(headers["Cache-Control"], "no-store")
                self.assertEqual({path.name for path in service.store.jobs[job].directory.iterdir()},
                                 {".slimeybeats-hybrid-job", "out"})
            self.assertEqual(guests[0].request("DELETE", "/v1/jobs/" + jobs[0])[0], 200)
            self.assertEqual(owner.request("DELETE", "/v1/jobs/" + jobs[1])[0], 200)
            self.assertEqual(service.store.jobs, {})
