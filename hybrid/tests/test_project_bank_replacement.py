"""A new imported bank replaces source state; separation preserves its source."""

from copy import deepcopy
import unittest

from hybrid.tests.project_support import command, create_project, native_studio, project_service, upload
from hybrid.tests.test_project_studio_state import float_signal, samples
from hybrid.tests.test_project_http import seed_asset


class ProjectBankReplacementTests(unittest.TestCase):
    def import_bank(self, client, manifest, amplitudes):
        ids = [upload(client, manifest["id"], float_signal(amplitude), role="stem", label=kind, kind=kind)
               for amplitude, kind in zip(amplitudes, ("vocals", "drums"))]
        result = command(client, manifest, "import_stems", upload_ids=ids)
        self.assertEqual(result["command"]["state"], "completed", result["command"])
        return result

    def pcm(self, client, manifest, audio_id):
        status, _, content = client.request("GET", f"/v2/projects/{manifest['id']}/assets/{audio_id}/audio")
        self.assertEqual(status, 200)
        return samples(content)

    def test_new_bank_replaces_existing_source_and_stale_master_atomically(self):
        studio, ffmpeg = native_studio(self)
        with project_service(studio_cli=studio, ffmpeg=ffmpeg) as (_, client):
            manifest = self.import_bank(client, create_project(client), (0.02, 0.03))
            old_original = manifest["original_id"]
            old_pcm = self.pcm(client, manifest, old_original)
            manifest = command(client, manifest, "master", reference_id=None)
            manifest = command(client, manifest, "issues", audio_id=old_original)
            self.assertIsNotNone(manifest["master_id"])
            self.assertIsNotNone(manifest["report"])
            self.assertIsNotNone(manifest["issues"])
            previous = deepcopy(manifest)
            manifest = self.import_bank(client, manifest, (0.2, 0.3))
            replacement = deepcopy(manifest)
            self.assertNotEqual(manifest["original_id"], old_original)
            self.assertEqual(manifest["input_id"], manifest["original_id"])
            self.assertEqual(manifest["before_id"], manifest["original_id"])
            self.assertTrue(manifest["preview_is_mix"])
            self.assertIsNone(manifest["edit_origin_kind"])
            for field in ("master_id", "report", "issues", "spectrogram"):
                self.assertIsNone(manifest[field], field)
            expected = samples(float_signal(0.5))
            for audio_id in (manifest["original_id"], manifest["preview_id"]):
                actual = self.pcm(client, manifest, audio_id)
                self.assertEqual(len(actual), len(expected))
                self.assertLess(max(abs(a - b) for a, b in zip(actual, expected)), 1e-7)
            self.assertEqual(self.pcm(client, manifest, old_original), old_pcm)
            manifest = command(client, manifest, "history", redo=False)
            for field in ("original_id", "input_id", "before_id", "preview_id", "master_id", "report", "issues", "stems"):
                self.assertEqual(manifest[field], previous[field], field)
            manifest = command(client, manifest, "history", redo=True)
            for field in ("original_id", "input_id", "before_id", "preview_id", "master_id", "report", "issues", "stems"):
                self.assertEqual(manifest[field], replacement[field], field)

    def test_separation_commit_keeps_existing_source_instead_of_new_raw_sum(self):
        from hybrid.server.project_models import ProjectCommand
        with project_service() as (service, client):
            manifest = create_project(client)
            project, original = seed_asset(service, manifest["id"])
            raw_sum, preview, stem = "a" * 32, "b" * 32, "c" * 32
            base = project.assets[original]
            assets = {raw_sum: {**base, "id": raw_sum, "role": "original"},
                      preview: {**base, "id": preview, "role": "edited"},
                      stem: {**base, "id": stem, "role": "stem", "kind": "vocals"}}
            request = {"operation": "separate", "base_revision": 0, "audio_id": original}
            command_fixture = ProjectCommand("d" * 32, project, request, project.directory / ("command-" + "d" * 32))
            service.worker.project_worker.commit(command_fixture, (assets, {}, preview, {"_bank_original_id": raw_sum}))
            self.assertEqual((project.original_id, project.input_id, project.before_id), (original, original, original))
            self.assertEqual(project.preview_id, preview)
            self.assertTrue(project.preview_is_mix)
            self.assertNotIn(raw_sum, project.assets)
