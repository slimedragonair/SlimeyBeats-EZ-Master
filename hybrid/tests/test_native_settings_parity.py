"""Real CLI/HTTP evidence for editable float exchange and reference application."""

import json
import math
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest

from hybrid.tests.support import running_service
from hybrid.tests.test_audio_guard import custom_wav
from hybrid.tests.test_native_cli import native_tools


def float_fixture(rate=44100, channels=2, frames=44100, bright=False):
    samples = []
    for frame in range(frames):
        time = frame / rate
        for channel in range(channels):
            samples.append(sum(amplitude * math.sin(2 * math.pi * frequency * time + channel * 0.3)
                               for frequency, amplitude in ((90, 0.08), (440, 0.1),
                                                            (1800, 0.07), (6500, 0.12 if bright else 0.01))))
    payload = struct.pack("<" + "f" * len(samples), *samples)
    return custom_wav(encoding=3, bits=32, channels=channels, rate=rate, data=payload)


def float_pcm(data):
    chunks = {}
    offset = 12
    while offset < len(data):
        tag, size = struct.unpack_from("<4sI", data, offset)
        chunks[tag] = data[offset + 8:offset + 8 + size]
        offset += 8 + size + size % 2
    encoding, channels, rate, _, _, bits = struct.unpack_from("<HHIIHH", chunks[b"fmt "])
    if encoding == 0xFFFE:
        encoding = struct.unpack_from("<I", chunks[b"fmt "], 24)[0]
    return (encoding, channels, rate, bits), chunks[b"data"]


class NativeSettingsParityTests(unittest.TestCase):
    def test_reference_analysis_matches_reference_wav_and_changes_pcm(self):
        cli, _ = native_tools(self)
        with tempfile.TemporaryDirectory(prefix="hybrid-reference-parity-") as temporary:
            directory = Path(temporary)
            source, reference = directory / "input.wav", directory / "reference.wav"
            source.write_bytes(float_fixture())
            reference.write_bytes(float_fixture(bright=True))
            analysis_path = directory / "analysis.json"
            subprocess.run([str(cli), str(reference), "--analyze-only", "--report-json", str(analysis_path)],
                           check=True, capture_output=True, timeout=60)
            analysis = json.loads(analysis_path.read_text())
            # The browser getter serializes compact JSON; its full native shape
            # must fit the existing 16 KiB HTTP request and CLI file boundaries.
            analysis_path.write_text(json.dumps(analysis, separators=(",", ":")))
            self.assertLess(analysis_path.stat().st_size, 16384 - 512)
            outputs = {}
            for name, arguments in (("none", []), ("wav", ["--reference", str(reference)]),
                                    ("analysis", ["--reference-analysis", str(analysis_path)])):
                out = directory / name
                report = directory / (name + "-report.json")
                subprocess.run([str(cli), str(source), "--out", str(out), "--format", "wav32f",
                                "--no-dither", "--no-safety", "--threads", "4", "--report-json", str(report),
                                *arguments], check=True, capture_output=True, timeout=90)
                geometry, pcm = float_pcm((out / "input_master_32f.wav").read_bytes())
                self.assertEqual(geometry, (3, 2, 44100, 32))
                self.assertEqual(len(pcm), 44100 * 2 * 4)
                outputs[name] = pcm
                if name != "none":
                    self.assertIn("reference_match", [entry["module"] for entry in json.loads(report.read_text())["log"]])
            self.assertEqual(outputs["analysis"], outputs["wav"])
            self.assertNotEqual(outputs["analysis"], outputs["none"])

    def test_http_float_geometry_reference_and_settings_match_direct_cli(self):
        cli, ffmpeg = native_tools(self)
        data = float_fixture(rate=32000, channels=1, frames=32001)
        with tempfile.TemporaryDirectory(prefix="hybrid-settings-parity-") as temporary:
            directory = Path(temporary)
            source = directory / "input.wav"
            source.write_bytes(data)
            reference_path = directory / "reference.json"
            subprocess.run([str(cli), str(source), "--analyze-only", "--report-json", str(reference_path)],
                           check=True, capture_output=True, timeout=60)
            reference = json.loads(reference_path.read_text())
            reference["measured_db"][7] += 12
            reference_path.write_text(json.dumps(reference, separators=(",", ":")))
            settings = {"profile": "metal:0.6+edm:0.4", "mode": "loud", "target_lufs": -12,
                        "ceiling_dbtp": -2, "intensity": 0.6, "safety": False}
            with running_service(cli=cli, ffmpeg=ffmpeg, worker_timeout_seconds=120) as (service, client):
                for supplied in (None, reference):
                    status, _, snapshot = client.create(data, filename="browser-track.wav", format="wav32f",
                                                        reference_analysis=supplied, **settings)
                    self.assertEqual(status, 201, snapshot)
                    job_id = snapshot["id"]
                    client.upload(job_id, data)
                    client.request("POST", f"/v1/jobs/{job_id}/run", {})
                    result = client.wait(job_id, timeout=120)
                    self.assertEqual(result["state"], "completed", result)
                    audio = next(item for item in result["artifacts"] if item["name"] == "master.wav")
                    content = client.request("GET", f"/v1/jobs/{job_id}/artifacts/{audio['id']}")[2]
                    geometry, pcm = float_pcm(content)
                    self.assertEqual(geometry, (3, 1, 32000, 32))
                    self.assertEqual(len(pcm), 32001 * 4)
                    self.assertTrue(all(math.isfinite(number[0]) for number in struct.iter_unpack("<f", pcm)))
                    report_artifact = next(item for item in result["artifacts"] if item["name"] == "report.json")
                    report = client.request("GET", f"/v1/jobs/{job_id}/artifacts/{report_artifact['id']}")[2]
                    self.assertEqual(report["reference_supplied"], supplied is not None)
                    self.assertFalse(report["safety"])
                    self.assertEqual(report["intensity"], 0.6)
                    self.assertEqual({p.name for p in service.store.jobs[job_id].directory.iterdir()},
                                     {".slimeybeats-hybrid-job", "out"})
                    name = "reference" if supplied is not None else "none"
                    direct_settings = directory / (name + "-settings.json")
                    direct_out = directory / name
                    direct_settings.write_text(json.dumps({**settings, "formats": ["wav32f"],
                                                           "out_dir": str(direct_out), "dither": False}))
                    arguments = ["--reference-analysis", str(reference_path)] if supplied is not None else []
                    subprocess.run([str(cli), str(source), "--settings", str(direct_settings),
                                    "--threads", "4", "--report-json", str(directory / (name + "-report.json")),
                                    *arguments], check=True, capture_output=True, timeout=90)
                    self.assertEqual(pcm, float_pcm((direct_out / "input_master_32f.wav").read_bytes())[1])
                    self.assertEqual(client.request("DELETE", f"/v1/jobs/{job_id}")[0], 200)

    def test_settings_reference_conflicts_with_analysis_and_is_not_ignored(self):
        cli, _ = native_tools(self)
        with tempfile.TemporaryDirectory(prefix="hybrid-reference-conflict-") as temporary:
            directory = Path(temporary)
            settings = directory / "settings.json"
            settings.write_text(json.dumps({"reference": "existing-reference.wav"}))
            result = subprocess.run([str(cli), "--settings", str(settings), "--reference-analysis", "ref.json"],
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 2)
            self.assertIn("cannot be combined with a reference WAV in settings", result.stderr)
