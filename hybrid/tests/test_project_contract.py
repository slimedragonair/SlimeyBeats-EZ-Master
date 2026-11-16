"""v2 schemas, capability defaults, range semantics and safe projections."""

from dataclasses import replace
import io
import math
import os
from pathlib import Path
import struct
import sys
import tempfile
import unittest

from hybrid.server.config import Config, MIB
from hybrid.server.errors import ApiError, ProcessingError
from hybrid.server.pcm_decoder import FloatWavSink
from hybrid.server.project_limits import capabilities
from hybrid.server.project_limits import processing_config
from hybrid.server.project_media import byte_range
from hybrid.server.project_reports import numeric
from hybrid.server.project_requests import command_request, upload_request
from hybrid.server.project_settings import settings_projection
from hybrid.server.studio_driver import safe_file
from hybrid.tests.support import TOKEN


class ProjectContractTests(unittest.TestCase):
    def config(self, **options):
        return Config(cli=Path(sys.executable), token=TOKEN, data_dir=Path(tempfile.gettempdir()) / "unused-project-test", **options)

    def test_v2_limits_do_not_change_v1_and_overrides_are_validated(self):
        config = self.config()
        self.assertEqual((config.max_upload_bytes, config.max_decoded_bytes, config.max_audio_seconds), (512 * MIB, 512 * MIB, 1800))
        limits = capabilities(config)["limits"]
        self.assertEqual((limits["max_upload_bytes"], limits["max_decoded_bytes"], limits["max_audio_seconds"]), (2048 * MIB, 2048 * MIB, 3600))
        self.assertEqual(limits["max_project_disk_bytes"], 16 * 1024 * MIB)
        self.assertEqual(limits["preview_kbps"], 192)
        self.assertEqual(limits["separation_max_decoded_bytes"], 512 * MIB)
        view = processing_config(config)
        self.assertEqual((view.memory_bytes, view.cpu_seconds, view.worker_timeout_seconds), (16 * 1024 * MIB, 14400, 3600))
        self.assertEqual((config.memory_bytes, config.cpu_seconds, config.worker_timeout_seconds), (4 * 1024 * MIB, 900, 900))
        for field, value in (("v2_retention_seconds", 3601), ("v2_max_upload_bytes", True),
                             ("v2_max_decoded_bytes", -1), ("v2_preview_kbps", 321)):
            with self.subTest(field=field), self.assertRaises(ValueError):
                replace(config, **{field: value})

    def test_full_gui_projection_rejects_paths_and_unknown_nested_controls(self):
        bands = [{"type": "bell", "freq": 1000, "gain": -1, "q": 0.707, "on": True} for _ in range(7)]
        settings = {"profile": "metal:0.6+edm:0.4", "mode": "loud", "target_lufs": -14, "ceiling_dbtp": -1,
                    "intensity": 0.7, "safety": True, "manual_eq": bands, "eq_output_db": -1,
                    "eq_hold_loudness": False, "dither": False, "match_strength": 0.5, "match_loudness": True,
                    "match_width": False, "max_makeup_db": 12, "max_limiter_gr_db": 4, "output_sr": 44100,
                    "disabled": ["hf_rebuild", "multiband"], "modules": {"static_eq": {"enabled": True, "max_db": 3}}}
        clean = settings_projection(settings)
        self.assertEqual(clean["manual_eq"], bands)
        self.assertIsNot(clean["manual_eq"], bands)
        for bad in ({"out_dir": "/private"}, {"reference": "https://secret"}, {"formats": ["mp3"]},
                    {"manual_eq": bands + bands}, {"safety": 1}, {"intensity": math.nan},
                    {"modules": {"static_eq": {"path": "/private"}}}):
            with self.subTest(bad=bad), self.assertRaises(ApiError):
                settings_projection(bad)
        with self.assertRaises(ApiError):
            settings_projection({"modules": {"resonance": {"max_notches": 2.5}}})

    def test_command_options_do_not_accept_paths_or_inapplicable_payloads(self):
        config = self.config()
        for bad in ({"operation": "master", "base_revision": True},
                    {"operation": "master", "base_revision": 0, "voice": {}},
                    {"operation": "mix", "base_revision": 0, "stems": [{"path": "/private", "kind": "vocals"}]},
                    {"operation": "spectrogram", "base_revision": 0, "spectrogram": {"columns": 1024, "rows": 256}},
                    {"operation": "region_edit", "base_revision": 0, "region": {"start_seconds": 2, "end_seconds": 1, "operation": {"kind": "silence"}}}):
            with self.subTest(bad=bad), self.assertRaises(ApiError):
                command_request(bad, config)
        result = command_request({"operation": "bundle", "base_revision": 0, "audio_id": "a" * 32,
                                  "kind": "mastering", "include_stems": False}, config)
        self.assertFalse(result["include_stems"])
        upload_request({"filename": "source.wav", "bytes": 10, "role": "stem", "label": "Lead vocals"}, config)
        with self.assertRaises(ApiError):
            upload_request({"filename": "source.wav", "bytes": 10, "role": "stem", "label": "../../private"}, config)

    def test_range_exact_open_suffix_and_unsatisfiable(self):
        self.assertEqual(byte_range(None, 10), (0, 9, False))
        for value, expected in (("bytes=2-4", (2, 4, True)), ("bytes=2-", (2, 9, True)),
                                ("bytes=-3", (7, 9, True)), ("bytes=-99", (0, 9, True)), ("bytes=0-99", (0, 9, True))):
            self.assertEqual(byte_range(value, 10), expected)
        for value in ("bytes=", "bytes=-", "bytes=10-", "bytes=5-4", "bytes=-0", "bytes=0-1,3-4", "items=0-1"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                byte_range(value, 10)

    def test_numeric_reports_and_private_output_containment(self):
        clean = numeric({"path": "secret", "logs": ["secret"], "metrics": {"rms_dbfs": -12}, "notes": "private",
                         "C:/private/path": 1, "configuration": {"token": "secret"}})
        self.assertEqual(clean, {"metrics": {"rms_dbfs": -12}})
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            inside = root / "stage"
            inside.mkdir()
            private = root / "private.wav"
            private.write_bytes(b"x")
            with self.assertRaises(ProcessingError):
                safe_file(private, inside)
            output = inside / "audio.wav"
            output.write_bytes(b"x")
            self.assertEqual(safe_file(output, inside), output)

    def test_float_pipe_sink_preserves_geometry_and_rejects_overflow(self):
        view = type("Limits", (), {"max_decoded_bytes": 64, "max_audio_seconds": 3600})()
        fmt = struct.pack("<HHIIHH", 3, 2, 96000, 96000 * 8, 8, 32)
        header = b"RIFF\xff\xff\xff\xffWAVEfmt " + struct.pack("<I", 16) + fmt + b"data\xff\xff\xff\xff"
        stream = io.BytesIO()
        sink = FloatWavSink(stream, view)
        data = header + struct.pack("<f", 0.1) * 16
        for offset in range(0, len(data), 3):
            sink.consume(data[offset:offset + 3])
        sink.finish()
        self.assertEqual(struct.unpack_from("<I", stream.getvalue(), 24)[0], 96000)
        self.assertEqual(len(stream.getvalue()), 44 + 64)
        with self.assertRaises(ProcessingError):
            sink.consume(b"1234")

    @unittest.skipUnless(os.name == "posix", "Unix virtualenv symlink invocation")
    def test_stem_python_preserves_virtualenv_symlink_and_package_prefix(self):
        import subprocess
        import venv
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            env = root / "env"
            venv.create(env, with_pip=False, symlinks=True)
            python = env / "bin" / "python"
            models = root / "models"
            models.mkdir()
            config = self.config(stem_python=python, stem_models=models)
            self.assertEqual(config.stem_python, python)
            self.assertNotEqual(config.stem_python, python.resolve())
            actual = subprocess.check_output([str(config.stem_python), "-c", "import sys;print(sys.prefix)"], text=True).strip()
            self.assertEqual(Path(actual), env)
