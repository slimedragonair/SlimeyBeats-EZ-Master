"""Optional HF evidence survives transport without accepting arbitrary metadata."""

from copy import deepcopy
from pathlib import Path
from types import SimpleNamespace
import json
import sys
import unittest

from hybrid.server.config import Config
from hybrid.server.eq_suggestions import suggestion
from hybrid.server.errors import ApiError, ProcessingError
from hybrid.server.high_frequency import summary
from hybrid.server.project_references import reference_projection
from hybrid.server.project_reports import analysis
from hybrid.server.reference_analysis import project_reference
from hybrid.tests.reference_fixture import reference_analysis
from hybrid.tests.support import TOKEN


def evidence():
    return {"version": 1, "band_low_hz": 10000, "band_high_hz": 16000,
            "reference_low_hz": 2000, "reference_high_hz": 8000,
            "sampled_windows": 32, "active_windows": 32, "candidate_windows": 24, "candidate_groups": 3,
            "candidate_fraction": 0.75, "observed_candidate_seconds": 0.5,
            "mean_high_power_fraction": 0.35, "p90_high_to_presence_db": 7, "mean_high_flatness": 0.8,
            "excess_candidate": True, "suggested_cut_db": -1.5, "sampling_capped": False}


def bands():
    return [{"type": "high_shelf" if index == 6 else "bell", "freq": frequency,
             "gain": 0, "q": 0.707, "on": True}
            for index, frequency in enumerate((50, 120, 300, 800, 2000, 5000, 10000))]


class HighFrequencyContractTests(unittest.TestCase):
    def setUp(self):
        self.config = Config(cli=Path(sys.executable), token=TOKEN)

    def project(self, raw):
        project = SimpleNamespace(reference_id="reference", assets={"reference": {"analysis": raw}})
        return reference_projection(project, {}, self.config)

    def test_optional_summary_roundtrips_both_references_and_public_analysis_without_aliases(self):
        for supplied in ("absent", None, evidence()):
            raw = reference_analysis()
            raw["duration_s"] = 2
            if supplied != "absent":
                raw["spectrum"]["high_frequency"] = supplied
            before = deepcopy(raw)
            for clean in (project_reference(raw, self.config), self.project(raw), analysis(raw)):
                if supplied == "absent":
                    self.assertNotIn("high_frequency", clean["spectrum"])
                else:
                    self.assertEqual(clean["spectrum"]["high_frequency"], supplied)
                    if supplied is not None:
                        self.assertIsNot(clean["spectrum"]["high_frequency"], supplied)
                self.assertNotIn("private", json.dumps(clean))
            self.assertEqual(raw, before)

    def test_invalid_summary_is_rejected_by_both_reference_ingresses_and_public_analysis(self):
        mutations = (("version", True), ("version", 2), ("sampled_windows", -1), ("active_windows", 33),
                     ("candidate_windows", 33), ("candidate_windows", True), ("candidate_groups", 25),
                     ("band_low_hz", 18000), ("band_high_hz", 25000), ("reference_high_hz", 11000),
                     ("candidate_fraction", 1.1), ("candidate_fraction", 0.5), ("mean_high_power_fraction", -1),
                     ("mean_high_flatness", 2), ("p90_high_to_presence_db", float("nan")),
                     ("suggested_cut_db", float("inf")), ("suggested_cut_db", True),
                     ("observed_candidate_seconds", 3), ("excess_candidate", 1), ("excess_candidate", False),
                     ("suggested_cut_db", 1), ("suggested_cut_db", -2.6), ("sampling_capped", "false"),
                     ("path", "private/hf.wav"))
        for key, value in mutations:
            raw = reference_analysis()
            raw["duration_s"] = 2
            raw["spectrum"]["high_frequency"] = {**evidence(), key: value}
            with self.subTest(key=key, value=value):
                with self.assertRaises(ApiError):
                    project_reference(raw, self.config)
                with self.assertRaises(ApiError):
                    self.project(raw)
                with self.assertRaises(ProcessingError):
                    analysis(raw)
        for value in ({}, [], "private/hf.wav"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                summary(value, 48000, 1)

    def test_native_suggestion_uses_fixed_safe_notes_and_retains_exact_bands_and_evidence(self):
        raw = reference_analysis()
        raw["duration_s"] = 2
        high_frequency = evidence()
        high_frequency["sampling_capped"] = True
        raw["spectrum"]["high_frequency"] = high_frequency
        proposal = {"bands": bands(), "high_frequency": high_frequency,
                    "notes": ["private native path /private/source.wav"], "path": "/private/worker"}
        before = deepcopy(proposal)
        clean = suggestion(proposal, raw)
        self.assertEqual(set(clean), {"bands", "notes", "high_frequency"})
        self.assertEqual(clean["bands"], proposal["bands"])
        self.assertEqual(clean["high_frequency"], high_frequency)
        self.assertTrue(all(isinstance(note, str) for note in clean["notes"]))
        self.assertIn("observed windows", " ".join(clean["notes"]))
        self.assertNotIn("private", json.dumps(clean))
        self.assertEqual(proposal, before)
        legacy = suggestion({"bands": bands()}, raw)
        self.assertIsNone(legacy["high_frequency"])
        self.assertIn("unavailable", " ".join(legacy["notes"]))

    def test_invalid_suggestion_evidence_and_bands_never_fall_back_to_a_valid_proposal(self):
        for proposal in ({"bands": bands()[:-1]}, {"bands": bands(), "high_frequency": {}},
                         {"bands": bands(), "high_frequency": {**evidence(), "active_windows": True}}):
            with self.subTest(proposal=proposal), self.assertRaises(ProcessingError):
                suggestion(proposal, reference_analysis())

    def test_new_analysis_issue_keeps_its_fixed_text_and_numerical_severity(self):
        raw = reference_analysis()
        raw["issues"] = [{"key": "high_frequency_excess", "severity": 45, "detail": "private source path"}]
        clean = analysis(raw)
        self.assertEqual(clean["issues"][0]["key"], "high_frequency_excess")
        self.assertEqual(clean["issues"][0]["severity"], 45)
        self.assertNotIn("private", json.dumps(clean))
