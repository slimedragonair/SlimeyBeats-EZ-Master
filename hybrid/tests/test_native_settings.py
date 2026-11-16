"""Private generated-file ownership, native invocation and completion cleanup."""

import json
import os
from unittest.mock import patch
import unittest

from hybrid.server.errors import ProcessingError
from hybrid.server.native_settings import write_private_json
from hybrid.tests.reference_fixture import reference_analysis
from hybrid.tests.support import running_service, wav_bytes


class NativeSettingsTests(unittest.TestCase):
    def test_generated_settings_reference_and_float_artifacts(self):
        with running_service() as (service, client):
            data = wav_bytes()
            job_id = client.create(data, format="wav32f", safety=False, mode="loud", target_lufs=-11,
                                   ceiling_dbtp=-2, intensity=0.6, profile="metal:0+edm:1",
                                   reference_analysis=reference_analysis())[2]["id"]
            job = service.store.jobs[job_id]
            client.upload(job_id, data)

            def native(command, active_job, timeout):
                self.assertIs(active_job, job)
                self.assertNotIn("--reference", command)
                path = command[command.index("--settings") + 1]
                self.assertEqual(path.parent, job.directory)
                settings = json.loads(path.read_text())
                self.assertEqual(settings, {"profile": "edm", "mode": "loud", "target_lufs": -11,
                                           "ceiling_dbtp": -2, "intensity": 0.6, "safety": False,
                                           "formats": ["wav32f"], "out_dir": str(job.directory / "out"),
                                           "output_sr": 0, "dither": False})
                reference_path = command[command.index("--reference-analysis") + 1]
                self.assertEqual(reference_path.parent, job.directory)
                self.assertNotIn("private", reference_path.read_text())
                if os.name == "posix":
                    self.assertEqual(path.stat().st_mode & 0o777, 0o600)
                    self.assertEqual(reference_path.stat().st_mode & 0o777, 0o600)
                (job.directory / "out" / "input_master_32f.wav").write_bytes(b"float WAV fixture")
                (job.directory / "raw-report.json").write_text(json.dumps({"input": {}, "output": {}}))

            with patch.object(service.worker.runner, "run", side_effect=native):
                artifacts = service.worker.process(job)
            self.assertEqual({a["name"] for a in artifacts.values()}, {"master.wav", "report.json"})
            self.assertEqual({p.name for p in job.directory.iterdir()}, {".slimeybeats-hybrid-job", "out"})
            report = json.loads((job.directory / "out" / "report.json").read_text())
            self.assertTrue(report["reference_supplied"])
            self.assertFalse(report["safety"])

    def test_no_reference_keeps_defaults_and_failures_remove_private_files(self):
        with running_service() as (service, client):
            data = wav_bytes()
            for reference in (None, reference_analysis()):
                job_id = client.create(data, reference_analysis=reference)[2]["id"]
                job = service.store.jobs[job_id]
                client.upload(job_id, data)

                def failing(command, active_job, timeout):
                    settings = json.loads((job.directory / "settings.json").read_text())
                    self.assertTrue(settings["safety"])
                    self.assertTrue(settings["dither"])
                    self.assertEqual("--reference-analysis" in command, reference is not None)
                    raise ProcessingError("Controlled native failure")

                with patch.object(service.worker.runner, "run", side_effect=failing):
                    client.request("POST", f"/v1/jobs/{job_id}/run", {})
                    result = client.wait(job_id)
                self.assertEqual(result["state"], "failed")
                self.assertEqual([p.name for p in job.directory.iterdir()], [".slimeybeats-hybrid-job"])
                client.request("DELETE", f"/v1/jobs/{job_id}")

    def test_private_json_never_replaces_existing_file(self):
        with running_service() as (service, client):
            job = service.store.jobs[client.create(b"x")[2]["id"]]
            path = job.directory / "settings.json"
            path.write_text("keep")
            with self.assertRaises(FileExistsError):
                write_private_json(path, {})
            self.assertEqual(path.read_text(), "keep")
