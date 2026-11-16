"""HTTP report Analysis must deserialize in the shared native/Live EQ type."""

from copy import deepcopy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from hybrid.server.native_settings import write_private_json
from hybrid.server.config import PROFILES
from hybrid.server.project_reports import ANALYSIS_ISSUE_TEXT
from hybrid.tests.project_support import command, create_project, native_studio, project_service, upload
from hybrid.tests.reference_fixture import reference_analysis
from hybrid.tests.support import wav_bytes
from hybrid.tests.test_project_studio_state import float_signal


class ProjectReportContractTests(unittest.TestCase):
    def test_returned_report_analyses_deserialize_as_real_native_analysis(self):
        studio, ffmpeg = native_studio(self)
        with project_service(studio_cli=studio, ffmpeg=ffmpeg) as (service, client):
            manifest = create_project(client)
            source_data = float_signal(1.2, seconds=0.4)
            source_id = upload(client, manifest["id"], source_data)
            manifest = command(client, manifest, "import", upload_ids=[source_id])
            manifest = command(client, manifest, "analyze", audio_id=manifest["original_id"])
            source_analysis = manifest["analysis"]
            warnings = {issue["key"]: issue for issue in source_analysis["issues"]}
            self.assertGreater(source_analysis["metrics"]["clipped_samples"], 0)
            self.assertIn("clipping", warnings)
            self.assertGreater(warnings["clipping"]["severity"], 0)
            self.assertEqual(warnings["clipping"]["detail"], ANALYSIS_ISSUE_TEXT["clipping"])
            manifest = command(client, manifest, "master", reference_id=None)
            self.assertEqual(manifest["command"]["state"], "completed", manifest["command"])
            public_report = manifest["report"]
            self.assertNotIn(str(service.storage.root), json.dumps(public_report))
            # Request.reference_analysis and LiveEq::new deserialize the same
            # mastercraft::analysis::Analysis. Pass the exact HTTP values with
            # no repair/default injection in this test's consumer.
            with tempfile.TemporaryDirectory(prefix="hybrid-report-serde-") as directory:
                root = Path(directory).resolve()
                source = root / "source.wav"
                source.write_bytes(source_data)
                for field, public_analysis in (("analysis", source_analysis), ("input", public_report["input"]), ("output", public_report["output"])):
                    out = root / ("out-" + field)
                    request_path = root / ("request-" + field + ".json")
                    response_path = out / "response.json"
                    write_private_json(request_path, {"api_version": 2, "operation": "master", "project_root": str(root),
                        "input": str(source), "out_dir": str(out),
                        "settings": {"intensity": 0, "safety": False, "formats": ["wav32f"]},
                        "reference_analysis": public_analysis})
                    process = subprocess.run([str(studio), "--request", str(request_path), "--response", str(response_path),
                                              "--threads", "4"], capture_output=True, text=True, timeout=30)
                    self.assertEqual(process.returncode, 0, field + ": " + process.stderr[-600:])
                    self.assertEqual(json.loads(response_path.read_text(encoding="utf-8"))["api_version"], 2)
                    self.assertIn(public_analysis["suggested_profile"], PROFILES)
                    for issue in public_analysis["issues"]:
                        self.assertEqual(set(issue), {"key", "severity", "detail"})
                        self.assertEqual(issue["detail"], ANALYSIS_ISSUE_TEXT[issue["key"]])
                        self.assertTrue(0 <= issue["severity"] <= 100)
            self.assertIsInstance(public_report["notes"], list)
            self.assertIsInstance(public_report["log"], list)
            self.assertIsInstance(public_report["exports"], list)
            self.assertEqual(public_report["profile"], "Neutral")

    def test_public_report_defaults_keep_text_private_and_do_not_mutate_analysis(self):
        from hybrid.server.project_reports import report
        raw = {"input": reference_analysis(), "output": reference_analysis(),
               "target_lufs": -14, "ceiling_dbtp": -1, "processing_seconds": 0.5,
               "limiter": {"iterations": 1, "reachable": True},
               "file": "private-file", "configuration": {"path": "private-config"}, "profile": "private-profile",
               "notes": ["private-note"], "log": [{"module": "private-module", "detail": {"path": "private-log"}}],
               "exports": [{"path": "private-export"}]}
        before = deepcopy(raw)
        clean = report(raw, "metal")
        self.assertEqual(clean["profile"], "Metal")
        self.assertEqual((clean["notes"], clean["log"], clean["exports"]), ([], [], []))
        self.assertEqual(clean["input"]["suggested_profile"], "neutral")
        self.assertEqual(clean["output"]["issues"], [])
        self.assertEqual(clean["output"]["spectrum"], raw["output"]["spectrum"])
        self.assertNotIn("private", json.dumps(clean))
        self.assertEqual(raw, before)

    def test_detector_allowlist_preserves_warnings_without_foreign_text(self):
        from hybrid.server.project_reports import analysis
        raw = reference_analysis()
        raw["suggested_profile"] = "metal"
        raw["issues"] = [
            {"key": "clipping", "severity": 90, "detail": "foreign-canary /private/source.wav"},
            {"key": "noise", "severity": 0.5, "detail": "foreign-canary"},
            {"key": "foreign-canary", "severity": 100, "detail": "foreign-canary"},
            {"key": "mud", "severity": True, "detail": "foreign-canary"},
            {"key": "stereo", "severity": 101, "detail": "foreign-canary"},
        ]
        before = deepcopy(raw)
        clean = analysis(raw)
        self.assertEqual(clean["suggested_profile"], "metal")
        self.assertEqual({issue["key"] for issue in clean["issues"]}, {"clipping", "noise"})
        self.assertNotIn("foreign-canary", json.dumps(clean))
        self.assertNotIn("/private", json.dumps(clean))
        self.assertEqual(raw, before)
        raw["suggested_profile"] = "foreign-canary"
        self.assertEqual(analysis(raw)["suggested_profile"], "neutral")
