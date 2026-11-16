"""Strict public settings parity, reference projection and pre-reservation rejection."""

from dataclasses import replace
import json
from pathlib import Path
import sys
import unittest

from hybrid.server.config import Config
from hybrid.server.errors import ApiError
from hybrid.server.requests import parse_master_request
from hybrid.tests.reference_fixture import reference_analysis
from hybrid.tests.support import TOKEN, running_service


class MasterSettingsTests(unittest.TestCase):
    def setUp(self):
        self.config = Config(cli=Path(sys.executable), token=TOKEN)
        self.base = {"filename": "browser-track.wav", "bytes": 48, "consent": True}

    def parse(self, **fields):
        return parse_master_request({**self.base, **fields}, self.config)

    def test_defaults_and_normal_ui_settings(self):
        request = self.parse()
        self.assertTrue(request.safety)
        self.assertIsNone(request.reference_analysis)
        request = self.parse(profile="metal:0.6+edm:0.4", mode="loud", target_lufs=-10,
                             ceiling_dbtp=-2, intensity=0.4, safety=False, format="wav32f",
                             reference_analysis=None)
        self.assertEqual((request.profile, request.mode, request.target_lufs, request.ceiling_dbtp,
                          request.intensity, request.safety, request.format),
                         ("metal:0.6+edm:0.4", "loud", -10, -2, 0.4, False, "wav32f"))
        for expression, expected in (("metal+edm", "metal:1.0+edm:1.0"),
                                     ("metal:0+edm:1", "edm"), ("metal:1+edm:0", "metal"),
                                     ("metal:0.5", "metal"), ("metal:1e+0+edm:0", "metal")):
            self.assertEqual(self.parse(profile=expression).profile, expected)

    def test_rejects_paths_custom_profiles_bad_blends_and_nonboolean_safety(self):
        expressions = ("../metal", "custom:metal", "neutral+../metal", "metal+rock+edm",
                       "combo:metal:0.5+edm:0.5", "metal:-0.1+edm:1", "metal:1.1+edm:0",
                       "metal:NaN+edm:1", "metal:inf+edm:1", "metal:1e999+edm:1",
                       "metal:0+edm:0", "metal:0", "metal::1", "metal+", "metal :1", "")
        for expression in expressions:
            with self.subTest(expression=expression), self.assertRaises(ApiError):
                self.parse(profile=expression)
        for safety in (None, 0, 1, "true", {}, []):
            with self.subTest(safety=safety), self.assertRaises(ApiError):
                self.parse(safety=safety)

    def test_copies_full_reference_but_discards_all_reference_text(self):
        original = reference_analysis()
        request = self.parse(reference_analysis=original)
        projected = request.reference_analysis
        self.assertNotIn("private", json.dumps(projected))
        self.assertEqual(projected["issues"], [])
        self.assertEqual(projected["suggested_profile"], "neutral")
        original["measured_db"][0] = 90
        original["valid"][0] = False
        original["metrics"]["dc_offset"][0] = 0.5
        self.assertEqual(projected["measured_db"][0], 0)
        self.assertTrue(projected["valid"][0])
        self.assertEqual(projected["metrics"]["dc_offset"], [0])

    def test_rejects_invalid_reference_before_job_or_directory_creation(self):
        mutations = (("measured_db", []), ("measured_db", [0] * 11),
                     ("deviation_db", [0] * 9), ("valid", [1] * 10),
                     ("channels", True), ("channels", 3), ("sample_rate", 1),
                     ("duration_s", 0), ("duration_s", 1801),
                     ("metrics.correlation", 1.1), ("metrics.dc_offset", []),
                     ("metrics.clipped_samples", -1), ("metrics.clipped_samples", 48001),
                     ("metrics.integrated_lufs", None), ("metrics.rms_dbfs", 10 ** 500),
                     ("spectrum.freqs", [20] * 240), ("spectrum.db", []),
                     ("spectrum.octave_power", [-1] * 10), ("spectrum.cutoff_hz", 48000),
                     ("spectrum.quiet_hf_flatness", 2), ("spectrum.noise_candidate", 1),
                     ("spectrum.sibilance_spike_fraction", -1), ("path", "private/path"))
        cases = [{}, [], "private/path"]
        for field, replacement in mutations:
            value = reference_analysis()
            parent = value
            pieces = field.split(".")
            for key in pieces[:-1]:
                parent = parent[key]
            parent[pieces[-1]] = replacement
            cases.append(value)
        missing = reference_analysis()
        del missing["metrics"]["plr_db"]
        cases.append(missing)
        with running_service() as (service, client):
            for index, value in enumerate(cases):
                with self.subTest(index=index):
                    status, _, response = client.create(reference_analysis=value)
                    self.assertEqual(status, 400, response)
                    self.assertNotIn("private", json.dumps(response))
                    self.assertFalse(service.store.jobs)
                    self.assertFalse(list(service.storage.root.glob("job-*")))

    def test_reference_finiteness_size_and_operator_limits(self):
        for field in ("measured_db", "deviation_db"):
            for number in (True, float("nan"), float("inf"), -float("inf")):
                value = reference_analysis()
                value[field][0] = number
                with self.subTest(field=field, number=number), self.assertRaises(ApiError):
                    self.parse(reference_analysis=value)
        value = reference_analysis()
        value["issues"] = ["x" * 16384]
        with self.assertRaises(ApiError):
            self.parse(reference_analysis=value)
        for config in (replace(self.config, max_audio_seconds=1),
                       replace(self.config, max_decoded_bytes=100)):
            value = reference_analysis()
            value["duration_s"] = 2
            with self.assertRaises(ApiError):
                parse_master_request({**self.base, "reference_analysis": value}, config)
