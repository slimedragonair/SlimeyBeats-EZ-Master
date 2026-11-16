"""Real PCM regressions for complete-bank previews and audio-only undo history."""

from array import array
from copy import deepcopy
import io
import math
import struct
import sys
import threading
import unittest
from unittest.mock import patch

from hybrid.tests.project_support import command, create_project, native_studio, project_service, upload
from hybrid.tests.support import process_alive, wait_until

HALF_GAIN_DB = 20 * math.log10(0.5)
DURATION = 1.2


def float_signal(amplitude, seconds=DURATION):
    rate = 48000
    values = array("f", (amplitude * math.sin(index * 2 * math.pi * 440 / rate)
                         for index in range(round(rate * seconds))))
    if sys.byteorder != "little":
        values.byteswap()
    payload = values.tobytes()
    fmt = struct.pack("<HHIIHH", 3, 1, rate, rate * 4, 4, 32)
    return b"RIFF" + struct.pack("<I", 36 + len(payload)) + b"WAVEfmt " + struct.pack("<I", 16) + fmt + b"data" + struct.pack("<I", len(payload)) + payload


def samples(data):
    source = io.BytesIO(data)
    source.seek(12)
    while True:
        tag, size = struct.unpack("<4sI", source.read(8))
        if tag == b"data":
            result = array("f")
            result.frombytes(source.read(size))
            if sys.byteorder != "little":
                result.byteswap()
            return result
        source.seek(size + size % 2, 1)


class ProjectStudioStateTests(unittest.TestCase):
    def bank(self, client):
        manifest = create_project(client)
        uploads = [upload(client, manifest["id"], float_signal(amplitude), role="stem", label=kind, kind=kind)
                   for amplitude, kind in ((0.1, "vocals"), (0.2, "drums"))]
        manifest = command(client, manifest, "import_stems", upload_ids=uploads)
        self.completed(manifest)
        return manifest

    def completed(self, manifest):
        self.assertEqual(manifest["command"]["state"], "completed", manifest["command"])

    def audio(self, client, manifest, audio_id=None):
        route = f"/v2/projects/{manifest['id']}/assets/{audio_id or manifest['preview_id']}/audio"
        status, _, content = client.request("GET", route)
        self.assertEqual(status, 200)
        return samples(content)

    def near(self, actual, expected):
        self.assertEqual(len(actual), len(expected))
        self.assertLessEqual(max(abs(a - b) for a, b in zip(actual, expected)), 1e-7)

    def test_region_stem_edit_commits_full_mix_and_readonly_preserves_undo_redo(self):
        studio, ffmpeg = native_studio(self)
        with project_service(studio_cli=studio, ffmpeg=ffmpeg) as (service, client):
            manifest = self.bank(client)
            root = manifest["original_id"]
            stems = {stem["kind"]: stem["id"] for stem in manifest["stems"]}
            vocals, drums = self.audio(client, manifest, stems["vocals"]), self.audio(client, manifest, stems["drums"])
            controls = deepcopy(manifest["controls"])
            controls["protect_peaks"] = False
            manifest = command(client, manifest, "mix", mix=controls)
            self.completed(manifest)
            previous = deepcopy(manifest)
            manifest = command(client, manifest, "region_edit", audio_id=stems["vocals"], region={
                "start_seconds": 0, "end_seconds": DURATION, "operation": {"kind": "gain", "db": HALF_GAIN_DB}})
            self.completed(manifest)
            edited = deepcopy(manifest)
            edited_stems = {stem["kind"]: stem["id"] for stem in manifest["stems"]}
            self.assertNotEqual(edited_stems["vocals"], stems["vocals"])
            self.assertEqual(edited_stems["drums"], stems["drums"])
            self.assertEqual(manifest["controls"], controls)
            self.assertEqual((manifest["before_id"], manifest["preview_is_mix"], manifest["edit_origin_kind"]), (root, True, None))
            self.near(self.audio(client, manifest), [0.5 * vocal + drum for vocal, drum in zip(vocals, drums)])
            self.near(self.audio(client, manifest, stems["vocals"]), vocals)
            project = service.store.projects.projects[manifest["id"]]
            history = deepcopy(project.undo)
            manifest = command(client, manifest, "issues", audio_id=edited_stems["vocals"])
            manifest = command(client, manifest, "export", audio_id=manifest["preview_id"], format="wav32f")
            self.completed(manifest)
            route = f"/v2/projects/{manifest['id']}/artifacts/{manifest['command']['artifact_id']}"
            self.near(samples(client.request("GET", route)[2]), [0.5 * vocal + drum for vocal, drum in zip(vocals, drums)])
            self.assertEqual(project.undo, history)
            manifest = command(client, manifest, "history", redo=False)
            self.assertEqual(manifest["preview_id"], previous["preview_id"])
            self.assertEqual(manifest["stems"], previous["stems"])
            self.near(self.audio(client, manifest), [vocal + drum for vocal, drum in zip(vocals, drums)])
            history, redo = deepcopy(project.undo), deepcopy(project.redo)
            for operation in ("analyze", "issues", "spectrogram", "suggest", "export", "bundle"):
                fields = {"audio_id": manifest["preview_id"]}
                if operation in ("export", "bundle"):
                    fields["format"] = "wav32f"
                manifest = command(client, manifest, operation, **fields)
                self.completed(manifest)
                self.assertEqual((project.undo, project.redo), (history, redo), operation)
                self.assertEqual(manifest["preview_id"], previous["preview_id"])
            manifest = command(client, manifest, "history", redo=True)
            self.assertEqual((manifest["preview_id"], manifest["stems"]), (edited["preview_id"], edited["stems"]))
            self.assertEqual((manifest["before_id"], manifest["preview_is_mix"], manifest["edit_origin_kind"]), (root, True, None))

    def test_voice_replace_uses_current_bank_controls_and_history_restores_ab(self):
        studio, ffmpeg = native_studio(self)
        with project_service(studio_cli=studio, ffmpeg=ffmpeg) as (_, client):
            manifest = self.bank(client)
            root = manifest["original_id"]
            stems = {stem["kind"]: stem["id"] for stem in manifest["stems"]}
            vocals, drums = self.audio(client, manifest, stems["vocals"]), self.audio(client, manifest, stems["drums"])
            controls = deepcopy(manifest["controls"])
            controls["protect_peaks"] = False
            for track in controls["tracks"]:
                if track["kind"] == "drums":
                    track["gain_db"] = HALF_GAIN_DB
            manifest = command(client, manifest, "mix", mix=controls)
            manifest = command(client, manifest, "voice_cleanup", audio_id=stems["vocals"], voice={
                "highpass_hz": 0, "noise_reduction": 0, "deess": 0, "output_gain_db": HALF_GAIN_DB,
                "hold_loudness": False, "protect_peaks": False})
            self.completed(manifest)
            solo = deepcopy(manifest)
            self.assertEqual((manifest["before_id"], manifest["preview_is_mix"], manifest["edit_origin_kind"]), (stems["vocals"], False, "vocals"))
            self.near(self.audio(client, manifest), [0.5 * value for value in vocals])
            manifest = command(client, manifest, "replace", audio_id=stems["vocals"])
            self.completed(manifest)
            self.assertNotEqual(manifest["preview_id"], solo["preview_id"])
            self.assertEqual(manifest["controls"], controls)
            self.assertEqual(next(stem["id"] for stem in manifest["stems"] if stem["kind"] == "vocals"), solo["preview_id"])
            self.assertEqual((manifest["before_id"], manifest["preview_is_mix"], manifest["edit_origin_kind"]), (root, True, None))
            expected = [0.5 * vocal + 0.5 * drum for vocal, drum in zip(vocals, drums)]
            self.near(self.audio(client, manifest), expected)
            manifest = command(client, manifest, "issues", audio_id=manifest["preview_id"])
            manifest = command(client, manifest, "export", audio_id=manifest["preview_id"], format="wav32f")
            route = f"/v2/projects/{manifest['id']}/artifacts/{manifest['command']['artifact_id']}"
            self.near(samples(client.request("GET", route)[2]), expected)
            manifest = command(client, manifest, "history", redo=False)
            self.assertEqual((manifest["preview_id"], manifest["stems"]), (solo["preview_id"], solo["stems"]))
            self.assertEqual((manifest["before_id"], manifest["preview_is_mix"], manifest["edit_origin_kind"]), (stems["vocals"], False, "vocals"))
            self.near(self.audio(client, manifest), [0.5 * value for value in vocals])

    def test_cancel_during_second_bank_pass_keeps_previous_committed_state(self):
        studio, ffmpeg = native_studio(self)
        with project_service(studio_cli=studio, ffmpeg=ffmpeg) as (service, client):
            manifest = self.bank(client)
            old = deepcopy(manifest)
            stem = next(stem for stem in manifest["stems"] if stem["kind"] == "vocals")
            worker = service.worker.project_worker
            invoke = worker.driver.invoke
            second = threading.Event()
            def controlled(item, request):
                if item.request["operation"] == "region_edit" and request["operation"] == "mix":
                    second.set()
                    code = "import os,pathlib,time;pathlib.Path('bank-mix.pid').write_text(str(os.getpid()));time.sleep(30)"
                    worker.runner.run([sys.executable, "-c", code], item, 40)
                return invoke(item, request)
            with patch.object(worker.driver, "invoke", side_effect=controlled):
                base = "/v2/projects/" + manifest["id"]
                self.assertEqual(client.request("POST", base + "/commands", {"operation": "region_edit", "base_revision": manifest["revision"],
                    "audio_id": stem["id"], "region": {"start_seconds": 0, "end_seconds": DURATION,
                                                        "operation": {"kind": "gain", "db": HALF_GAIN_DB}}})[0], 202)
                self.assertTrue(second.wait(10))
                project = service.store.projects.projects[manifest["id"]]
                pidfile = project.directory / "bank-mix.pid"
                wait_until(pidfile.exists)
                pid = int(pidfile.read_text())
                result = client.request("POST", base + "/cancel", {})[2]
            self.assertEqual(result["command"]["state"], "cancelled")
            for key in ("revision", "original_id", "preview_id", "stems", "controls", "before_id", "preview_is_mix", "edit_origin_kind"):
                self.assertEqual(result[key], old[key], key)
            self.assertFalse(process_alive(pid))
            self.assertFalse(project.command.stage.exists())

    def test_native_mix_rejection_preserves_bank_preview_ab_and_history(self):
        studio, ffmpeg = native_studio(self)
        with project_service(studio_cli=studio, ffmpeg=ffmpeg) as (service, client):
            manifest = self.bank(client)
            short = upload(client, manifest["id"], float_signal(0.05, seconds=DURATION / 2))
            manifest = command(client, manifest, "import", upload_ids=[short], as_preview=True)
            self.completed(manifest)
            previous = deepcopy(manifest)
            project = service.store.projects.projects[manifest["id"]]
            history = deepcopy(project.undo)
            target = next(stem["id"] for stem in manifest["stems"] if stem["kind"] == "vocals")
            manifest = command(client, manifest, "replace", audio_id=target)
            self.assertEqual((manifest["command"]["state"], manifest["command"]["error_code"]), ("failed", "processing_failed"))
            for key in ("revision", "original_id", "preview_id", "stems", "controls", "before_id", "preview_is_mix", "edit_origin_kind"):
                self.assertEqual(manifest[key], previous[key], key)
            self.assertEqual(project.undo, history)
            self.assertFalse(project.command.stage.exists())

    def test_doctor_and_spectral_stem_previews_keep_explicit_ab_origin(self):
        studio, ffmpeg = native_studio(self)
        with project_service(studio_cli=studio, ffmpeg=ffmpeg) as (_, client):
            manifest = self.bank(client)
            bank = deepcopy(manifest["stems"])
            target = next(stem["id"] for stem in bank if stem["kind"] == "vocals")
            for operation, fields in (("repair", {"repair": {"action": "reduce_low_mids", "start_seconds": 0,
                                                               "end_seconds": DURATION, "strength": 0}}),
                                      ("spectral_repair", {"spectral": {"start_seconds": 0, "end_seconds": DURATION,
                                                                       "low_hz": 100, "high_hz": 1000,
                                                                       "reduction_db": 0, "mode": "attenuate"}})):
                manifest = command(client, manifest, operation, audio_id=target, **fields)
                self.completed(manifest)
                self.assertEqual(manifest["stems"], bank)
                self.assertEqual((manifest["before_id"], manifest["preview_is_mix"], manifest["edit_origin_kind"]), (target, False, "vocals"))
