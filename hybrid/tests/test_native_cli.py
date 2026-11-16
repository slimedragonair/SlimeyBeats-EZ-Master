"""HTTP-to-native smoke: absent supplied executables are explicitly skipped."""

import io
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import unittest
import wave

from hybrid.tests.support import running_service, wav_bytes
from hybrid.tests.test_audio_guard import custom_wav


def native_tools(test):
    supplied = os.environ.get("HYBRID_TEST_CLI")
    if not supplied:
        if os.environ.get("CI") or os.environ.get("GITHUB_ACTIONS"):
            test.fail("CI must supply HYBRID_TEST_CLI for the actual native smoke test")
        test.skipTest("Actual CLI not supplied: run python -m hybrid.tests --cli ABS")
    ffmpeg_value = os.environ.get("HYBRID_TEST_FFMPEG") or shutil.which("ffmpeg")
    return Path(supplied).resolve(strict=True), Path(ffmpeg_value).resolve(strict=True) if ffmpeg_value else None


class NativeCliTests(unittest.TestCase):
    def test_real_cli_wav_and_mp3_artifacts(self):
        cli, ffmpeg = native_tools(self)
        if (os.environ.get("CI") or os.environ.get("GITHUB_ACTIONS")) and ffmpeg is None:
            self.fail("CI must install FFmpeg for actual MP3 qualification")
        with running_service(cli=cli, ffmpeg=ffmpeg, worker_timeout_seconds=120,
                             max_job_disk_bytes=128 * 1024 * 1024) as (service, client):
            data = wav_bytes(2)
            formats = ["wav24", "mp3"] if ffmpeg else ["wav24"]
            for fmt in formats:
                with self.subTest(output_format=fmt):
                    status, _, snapshot = client.create(data, format=fmt)
                    self.assertEqual(status, 201)
                    job_id = snapshot["id"]
                    client.upload(job_id, data)
                    self.assertEqual(client.request("POST", f"/v1/jobs/{job_id}/run", {})[0], 200)
                    result = client.wait(job_id, timeout=120)
                    self.assertEqual(result["state"], "completed", result)
                    self.assertEqual(result["progress"], 1)
                    self.assertEqual(len(result["artifacts"]), 2)
                    for artifact in result["artifacts"]:
                        status, headers, content = client.request("GET", f"/v1/jobs/{job_id}/artifacts/{artifact['id']}")
                        self.assertEqual(status, 200)
                        self.assertEqual(headers["Content-Type"], artifact["content_type"])
                        self.assertIn('attachment; filename="', headers["Content-Disposition"])
                        if artifact["name"] == "report.json":
                            serialized = json.dumps(content)
                            for private in (str(service.storage.root), "out_dir", '"file"', '"path"', "configuration", "exports"):
                                self.assertNotIn(private, serialized)
                            self.assertIn("integrated_lufs", content["output"]["metrics"])
                        elif fmt == "wav24":
                            with wave.open(io.BytesIO(content)) as audio:
                                self.assertEqual(audio.getsampwidth(), 3)
                                self.assertEqual(audio.getnframes(), 96000)
                        else:
                            self.assertGreater(len(content), 1000)
                    self.assertEqual(client.request("DELETE", "/v1/jobs/" + job_id)[2], {"deleted": True})
                    self.assertFalse((service.storage.root / ("job-" + job_id)).exists())
            if ffmpeg:
                with tempfile.TemporaryDirectory(prefix="hybrid-mp3-fixture-") as temp:
                    source = Path(temp) / "fixture.wav"
                    encoded = Path(temp) / "fixture.mp3"
                    source.write_bytes(data)
                    subprocess.run([str(ffmpeg), "-v", "error", "-nostdin", "-i", str(source),
                                    "-c:a", "libmp3lame", str(encoded)], check=True, capture_output=True, timeout=30)
                    mp3 = encoded.read_bytes()
                status, _, snapshot = client.create(mp3, filename="song.mp3")
                self.assertEqual(status, 201)
                client.upload(snapshot["id"], mp3)
                client.request("POST", f"/v1/jobs/{snapshot['id']}/run", {})
                self.assertEqual(client.wait(snapshot["id"], timeout=120)["state"], "completed")
            else:
                self.skipTest("MP3 qualification not run: FFmpeg is not installed")

    def test_real_cli_float_wav_strips_metadata(self):
        cli, ffmpeg = native_tools(self)
        with running_service(cli=cli, ffmpeg=ffmpeg, max_job_disk_bytes=128 * 1024 * 1024) as (_, client):
            private = b"private-host-name/path/to/audio"
            if len(private) % 2:
                private += b"\0"
            metadata = b"JUNK" + struct.pack("<I", len(private)) + private
            samples = struct.pack("<f", 0.25) * 48000
            data = custom_wav(encoding=3, bits=32, data=samples, metadata=metadata)
            job = client.create(data)[2]["id"]
            client.upload(job, data)
            client.request("POST", f"/v1/jobs/{job}/run", {})
            result = client.wait(job)
            self.assertEqual(result["state"], "completed", result)
            for artifact in result["artifacts"]:
                content = client.request("GET", f"/v1/jobs/{job}/artifacts/{artifact['id']}")[2]
                self.assertNotIn(private, content if isinstance(content, bytes) else json.dumps(content).encode())

    def test_mp3_overflow_and_disguised_playlist_are_rejected(self):
        cli, ffmpeg = native_tools(self)
        if ffmpeg is None:
            self.skipTest("MP3 decoder qualification requires FFmpeg")
        with tempfile.TemporaryDirectory(prefix="hybrid-mp3-overflow-") as temp:
            source, encoded = Path(temp) / "fixture.wav", Path(temp) / "fixture.mp3"
            source.write_bytes(wav_bytes(2))
            subprocess.run([str(ffmpeg), "-v", "error", "-nostdin", "-i", str(source),
                            "-c:a", "libmp3lame", str(encoded)], check=True, capture_output=True, timeout=30)
            long_mp3 = encoded.read_bytes()
        with running_service(cli=cli, ffmpeg=ffmpeg, max_audio_seconds=1,
                             max_job_disk_bytes=128 * 1024 * 1024) as (service, client):
            for data in (long_mp3, b"#EXTM3U\nfile:///etc/passwd\nhttp://127.0.0.1:9/private\n"):
                job = client.create(data, filename="song.mp3")[2]["id"]
                client.upload(job, data)
                client.request("POST", f"/v1/jobs/{job}/run", {})
                result = client.wait(job)
                self.assertEqual(result["state"], "failed", result)
                self.assertEqual(result["artifacts"], [])
                self.assertEqual([item.name for item in service.store.jobs[job].directory.iterdir()], [".slimeybeats-hybrid-job"])
