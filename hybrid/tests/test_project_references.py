"""Explicit per-master reference choice, project isolation and private full Analysis."""

from copy import deepcopy
import unittest
from unittest.mock import patch

from hybrid.server.errors import ApiError
from hybrid.server.project_references import reference_projection
from hybrid.server.project_requests import command_request
from hybrid.tests.project_support import command, create_project, native_studio, project_service, upload
from hybrid.tests.support import wav_bytes


class ProjectReferenceTests(unittest.TestCase):
    def test_null_omits_reference_and_explicit_prior_asset_survives_future_masters(self):
        studio, ffmpeg = native_studio(self)
        with project_service(studio_cli=studio, ffmpeg=ffmpeg) as (service, client):
            manifest = create_project(client)
            source = upload(client, manifest["id"], wav_bytes(0.4))
            manifest = command(client, manifest, "import", upload_ids=[source])
            for seconds in (0.2, 0.3):
                reference = upload(client, manifest["id"], wav_bytes(seconds), role="reference")
                manifest = command(client, manifest, "reference", upload_ids=[reference])
                self.assertEqual(manifest["command"]["state"], "completed", manifest["command"])
                if seconds == 0.2:
                    first_id = manifest["reference_id"]
            second_id = manifest["reference_id"]
            self.assertNotEqual(first_id, second_id)
            project = service.store.projects.projects[manifest["id"]]
            self.assertIn("spectrum", project.assets[first_id]["analysis"])
            public_reference = next(asset for asset in manifest["assets"] if asset["id"] == first_id)
            self.assertEqual(set(public_reference["analysis"]), {"metrics"})
            driver = service.worker.project_worker.driver
            invoke = driver.invoke
            captured = []
            def record(item, request):
                if request["operation"] == "master":
                    captured.append(deepcopy(request))
                return invoke(item, request)
            with patch.object(driver, "invoke", side_effect=record):
                for choice in (None, first_id, first_id, first_id, "omitted", None):
                    fields = {} if choice == "omitted" else {"reference_id": choice}
                    manifest = command(client, manifest, "master", settings={"intensity": 0.5}, **fields)
                    self.assertEqual(manifest["command"]["state"], "completed", manifest["command"])
                    self.assertEqual(manifest["reference_id"], second_id)
            self.assertNotIn("reference_analysis", captured[0])
            self.assertNotIn("reference_analysis", captured[-1])
            for request in captured[1:4]:
                self.assertEqual(request["reference_analysis"]["duration_s"], 0.2)
                self.assertEqual(len(request["reference_analysis"]["spectrum"]["freqs"]), 240)
            self.assertEqual(captured[4]["reference_analysis"]["duration_s"], 0.3)
            self.assertIn(first_id, project.assets)

    def test_reference_selector_is_master_only_and_never_borrows_another_project(self):
        with project_service() as (service, client):
            manifest = create_project(client)
            project = service.store.projects.projects[manifest["id"]]
            self.assertIsNone(reference_projection(project, {"reference_id": None}, service.config))
            with self.assertRaises(ApiError) as missing:
                reference_projection(project, {"reference_id": "a" * 32}, service.config)
            self.assertEqual(missing.exception.status, 404)
            project.assets["b" * 32] = {"analysis": {"metrics": {"rms_dbfs": -12}}}
            with self.assertRaises(ApiError) as invalid:
                reference_projection(project, {"reference_id": "b" * 32}, service.config)
            self.assertEqual((invalid.exception.status, invalid.exception.message), (409, "reference_analysis_required"))
            accepted = command_request({"operation": "master", "base_revision": 0, "reference_id": None}, service.config)
            self.assertIn("reference_id", accepted)
            self.assertIsNone(accepted["reference_id"])
            for invalid in ({"operation": "analyze", "base_revision": 0, "reference_id": None},
                            {"operation": "master", "base_revision": 0, "reference_id": "/private/reference.wav"}):
                with self.assertRaises(ApiError):
                    command_request(invalid, service.config)
