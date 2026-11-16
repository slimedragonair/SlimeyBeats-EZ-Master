"""Practical large-bank admission without full Analysis duplication in manifests."""

from copy import deepcopy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from hybrid.server.errors import ApiError
from hybrid.server.project_limits import MAX_ASSETS, MAX_STEMS, MAX_STORAGE_FILES, MAX_UPLOADS
from hybrid.server.project_models import public_asset
from hybrid.server.project_requests import command_request
from hybrid.server.storage import directory_bytes
from hybrid.tests.project_support import command, create_project, native_studio, project_service, upload
from hybrid.tests.test_project_studio_state import float_signal, samples


class LargeProjectBankTests(unittest.TestCase):
    def test_real_32_stem_import_keeps_complete_mix_and_bounded_public_metadata(self):
        studio, ffmpeg = native_studio(self)
        with project_service(studio_cli=studio, ffmpeg=ffmpeg) as (service, client):
            manifest = create_project(client)
            uploads = [upload(client, manifest["id"], float_signal(0.1 / 32, seconds=0.05),
                              role="stem", label=f"Track {index}", kind=f"part_{index}") for index in range(1, 33)]
            manifest = command(client, manifest, "import_stems", upload_ids=uploads)
            self.assertEqual(manifest["command"]["state"], "completed", manifest["command"])
            self.assertEqual(len(manifest["stems"]), 32)
            self.assertEqual({stem["kind"] for stem in manifest["stems"]}, {f"part_{index}" for index in range(1, 33)})
            self.assertEqual(len(manifest["controls"]["tracks"]), 32)
            self.assertEqual((manifest["before_id"], manifest["preview_is_mix"]), (manifest["original_id"], True))
            self.assertLess(len(json.dumps(manifest).encode()), 1024**2)
            private = service.store.projects.projects[manifest["id"]].assets[manifest["preview_id"]]["analysis"]
            self.assertIn("spectrum", private)
            public = next(asset for asset in manifest["assets"] if asset["id"] == manifest["preview_id"])
            self.assertEqual(set(public["analysis"]), {"metrics"})
            self.assertIn("spectrum", manifest["analysis"])
            route = f"/v2/projects/{manifest['id']}/assets/{manifest['preview_id']}/audio"
            actual = samples(client.request("GET", route)[2])
            expected = samples(float_signal(0.1, seconds=0.05))
            self.assertEqual(len(actual), len(expected))
            self.assertLess(max(abs(a - b) for a, b in zip(actual, expected)), 1e-7)

    def test_128_batch_admission_and_private_analysis_projection(self):
        with project_service() as (service, _):
            self.assertGreaterEqual(MAX_STEMS, 128)
            self.assertGreaterEqual(MAX_UPLOADS, 128)
            self.assertGreaterEqual(MAX_ASSETS, 256)
            self.assertGreaterEqual(MAX_STORAGE_FILES, 2048)
            ids = [f"{index:032x}" for index in range(128)]
            request = command_request({"operation": "import_stems", "base_revision": 0, "upload_ids": ids}, service.config)
            self.assertEqual(len(request["upload_ids"]), 128)
            with self.assertRaises(ApiError):
                command_request({"operation": "import_stems", "base_revision": 0,
                                 "upload_ids": [f"{index:032x}" for index in range(MAX_STEMS + 1)]}, service.config)
            full = {"metrics": {"integrated_lufs": -14, "sample_peak_dbfs": -1, "dc_offset": [0, 0]},
                    "spectrum": {"freqs": list(range(240)), "db": [-80] * 240},
                    "valid": [True] * 10, "measured_db": [0] * 10}
            asset = {"id": ids[0], "role": "stem", "name": "Track", "frames": 48000, "sample_rate": 48000,
                     "channels": 2, "duration_seconds": 1, "bytes": 384044, "content_type": "audio/wav",
                     "has_preview": True, "has_waveform": True, "analysis": full}
            before = deepcopy(asset)
            projected = [public_asset({**asset, "id": f"{index:032x}"}) for index in range(MAX_ASSETS)]
            self.assertLess(len(json.dumps(projected).encode()), 1024**2)
            self.assertEqual(asset, before)
            self.assertEqual(projected[0]["analysis"], {"metrics": full["metrics"]})
            from hybrid.server.project_limits import processing_config
            from hybrid.server.project_native_request import private_request
            command_fixture = type("Command", (), {"project": type("Project", (), {"directory": Path(tempfile.gettempdir())})(),
                                                    "stage": Path(tempfile.gettempdir()) / "unused-stage"})()
            native = private_request(command_fixture, processing_config(service.config), "mix", {})
            self.assertLessEqual(native["limits"]["max_bank_pcm_bytes"], 4 * 1024**3)

    def test_disk_watchdog_tolerates_removed_temporary_descendants_but_not_root(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            volatile = root / "native-temp"
            volatile.mkdir()
            (root / "kept.bin").write_bytes(b"12345")
            scandir = os.scandir
            def remove_during_scan(path):
                if Path(path) == volatile:
                    volatile.rmdir()
                return scandir(path)
            with patch("hybrid.server.storage.os.scandir", side_effect=remove_during_scan):
                self.assertEqual(directory_bytes(root, 10), 5)
            with self.assertRaises(FileNotFoundError):
                directory_bytes(root / "missing")
