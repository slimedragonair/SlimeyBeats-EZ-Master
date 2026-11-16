"""Actual native-worker/HTTP/codec integration, separate from mocked unit tests."""

import hashlib
from array import array
import io
import json
from pathlib import Path
import subprocess
import struct
import tempfile
import unittest
import zipfile

from hybrid.tests.project_support import command, create_project, native_studio, project_service, upload
from hybrid.tests.support import wav_bytes


class ProjectNativeTests(unittest.TestCase):
    def test_import_reference_master_render_bundle_range_and_rollback(self):
        studio, ffmpeg = native_studio(self)
        with project_service(studio_cli=studio, ffmpeg=ffmpeg) as (service, client):
            manifest = create_project(client)
            base = "/v2/projects/" + manifest["id"]
            original = upload(client, manifest["id"], wav_bytes(0.4))
            manifest = command(client, manifest, "import", upload_ids=[original])
            self.assertEqual(manifest["command"]["state"], "completed", manifest)
            self.assertEqual(manifest["revision"], 1)
            original_id = manifest["original_id"]
            source = service.store.projects.projects[manifest["id"]].assets[original_id]["path"]
            digest = hashlib.sha256(source.read_bytes()).digest()
            waveform = client.request("GET", base + f"/assets/{original_id}/waveform")[2]
            self.assertGreater(len(waveform["peaks"]), 0)
            self.assertEqual(len(waveform["channel_peaks"]), 1)
            preview_file = service.store.projects.projects[manifest["id"]].assets[original_id]["preview_path"]
            decoded_preview = subprocess.run([str(ffmpeg), "-v", "error", "-nostdin", "-i", str(preview_file),
                                              "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"],
                                             check=True, capture_output=True, timeout=30).stdout
            self.assertEqual(len(decoded_preview) // 8, 17640, "MP3 codec delay/padding must not shift the preview timeline")
            reference = upload(client, manifest["id"], wav_bytes(0.2), "reference")
            manifest = command(client, manifest, "reference", upload_ids=[reference])
            self.assertEqual(manifest["command"]["state"], "completed", manifest)
            manifest = command(client, manifest, "master", settings={"intensity": 0.5, "safety": True})
            self.assertEqual(manifest["command"]["state"], "completed", manifest)
            master_id = manifest["master_id"]
            master_path = service.store.projects.projects[manifest["id"]].assets[master_id]["path"]
            master_hash = hashlib.sha256(master_path.read_bytes()).digest()
            manifest = command(client, manifest, "render", settings={"eq_output_db": -3, "eq_hold_loudness": False}, format="wav16")
            self.assertEqual(manifest["command"]["state"], "completed", manifest)
            self.assertEqual(manifest["master_id"], master_id)
            self.assertNotEqual(manifest["preview_id"], master_id)
            self.assertEqual(hashlib.sha256(master_path.read_bytes()).digest(), master_hash)
            self.assertEqual(hashlib.sha256(source.read_bytes()).digest(), digest)
            preview = base + f"/assets/{manifest['preview_id']}/preview"
            status, headers, data = client.request("GET", preview, headers={"Range": "bytes=5-19"})
            self.assertEqual((status, len(data)), (206, 15))
            self.assertTrue(headers["Content-Range"].startswith("bytes 5-19/"))
            self.assertEqual(headers["Accept-Ranges"], "bytes")
            self.assertEqual(client.request("GET", preview, headers={"Range": "bytes=999999999-"})[0], 416)
            manifest = command(client, manifest, "bundle", audio_id=master_id, settings={"eq_output_db": -2},
                               format="wav24", kind="mastering", include_stems=False)
            self.assertEqual(manifest["command"]["state"], "completed", manifest)
            artifact_id = manifest["command"]["artifact_id"]
            status, _, archive = client.request("GET", base + "/artifacts/" + artifact_id)
            self.assertEqual(status, 200)
            with zipfile.ZipFile(io.BytesIO(archive)) as bundle:
                self.assertIn("master.wav", bundle.namelist())
                self.assertIn("settings.json", bundle.namelist())
                self.assertFalse(any(name.startswith("stems/") for name in bundle.namelist()))
            previous_revision, previous_id = manifest["revision"], manifest["original_id"]
            corrupt = upload(client, manifest["id"], b"not a valid WAV")
            failed = command(client, manifest, "import", upload_ids=[corrupt])
            self.assertEqual(failed["command"]["state"], "failed")
            self.assertEqual(failed["revision"], previous_revision)
            self.assertEqual(failed["original_id"], previous_id)
            self.assertEqual(failed["command"]["error_code"], "processing_failed")
            self.assertNotIn(str(service.storage.root), json.dumps(failed))
            replacement = upload(client, manifest["id"], wav_bytes(0.1))
            replaced = command(client, failed, "import", upload_ids=[replacement])
            self.assertEqual(replaced["command"]["state"], "completed", replaced)
            self.assertNotEqual(replaced["original_id"], previous_id)
            self.assertEqual(hashlib.sha256(source.read_bytes()).digest(), digest)
            undone = command(client, replaced, "history", redo=False)
            self.assertEqual(undone["original_id"], previous_id)
            redone = command(client, undone, "history", redo=True)
            self.assertEqual(redone["original_id"], replaced["original_id"])
            self.assertEqual(client.request("DELETE", base)[2], {"deleted": True})
            self.assertFalse(source.exists())

    def test_actual_edit_tools_clone_spectrogram_and_export_formats(self):
        studio, ffmpeg = native_studio(self)
        with project_service(studio_cli=studio, ffmpeg=ffmpeg) as (_, client):
            main = create_project(client)
            source = upload(client, main["id"], wav_bytes(0.3))
            main = command(client, main, "import", upload_ids=[source])
            self.assertEqual(main["command"]["state"], "completed", main)
            studio_project = create_project(client)
            manifest = command(client, studio_project, "clone", source_project_id=main["id"], audio_id=main["original_id"])
            self.assertEqual(manifest["command"]["state"], "completed", manifest)
            operations = [("analyze", {}), ("issues", {}),
                          ("region_edit", {"region": {"start_seconds": 0, "end_seconds": 0.1, "operation": {"kind": "gain", "db": -3}}}),
                          ("voice_cleanup", {"voice": {"noise_reduction": 0, "deess": 0}}),
                          ("spectral_repair", {"spectral": {"start_seconds": 0, "end_seconds": 0.1, "low_hz": 100,
                                                           "high_hz": 1000, "reduction_db": -6, "mode": "attenuate"}}),
                          ("repair", {"repair": {"action": "reduce_low_mids", "start_seconds": 0, "end_seconds": 0.1, "strength": 0.5}}),
                          ("spectrogram", {})]
            for operation, fields in operations:
                manifest = command(client, manifest, operation, **fields)
                self.assertEqual(manifest["command"]["state"], "completed", (operation, manifest))
            self.assertLess(len(json.dumps(manifest).encode()), 1024**2)
            for fmt in ("wav24", "wav16", "wav32f", "flac24", "flac16", "mp3"):
                manifest = command(client, manifest, "export", audio_id=manifest["preview_id"], format=fmt)
                self.assertEqual(manifest["command"]["state"], "completed", (fmt, manifest))
                artifact = manifest["artifacts"][-1]
                route = f"/v2/projects/{manifest['id']}/artifacts/{artifact['id']}"
                self.assertEqual(client.request("GET", route, headers={"Range": "bytes=-10"})[0], 206)

    def test_compressed_inputs_preserve_original_geometry_and_do_not_truncate(self):
        studio, ffmpeg = native_studio(self)
        with tempfile.TemporaryDirectory() as directory, project_service(studio_cli=studio, ffmpeg=ffmpeg) as (_, client):
            folder = Path(directory)
            source = folder / "source.wav"
            source.write_bytes(wav_bytes(0.3, rate=44100))
            manifest = create_project(client)
            for extension, codec in (("mp3", "libmp3lame"), ("flac", "flac"), ("m4a", "aac"),
                                     ("aiff", "pcm_s24be"), ("ogg", "libvorbis"), ("aac", "aac")):
                encoded = folder / ("encoded." + extension)
                subprocess.run([str(ffmpeg), "-v", "error", "-nostdin", "-i", str(source), "-c:a", codec, str(encoded)],
                               check=True, capture_output=True, timeout=30)
                upload_id = upload(client, manifest["id"], encoded.read_bytes(), filename="source." + extension)
                manifest = command(client, manifest, "import", upload_ids=[upload_id])
                self.assertEqual(manifest["command"]["state"], "completed", (extension, manifest))
                asset = next(asset for asset in manifest["assets"] if asset["id"] == manifest["original_id"])
                self.assertEqual((asset["sample_rate"], asset["channels"]), (44100, 1), extension)
                decoded = subprocess.run([str(ffmpeg), "-v", "error", "-nostdin", "-i", str(encoded),
                                          "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"], check=True,
                                         capture_output=True, timeout=30).stdout
                self.assertEqual(asset["frames"], len(decoded) // 4, extension)
            for extension in ("mp3", "m4a", "flac", "ogg"):
                upload_id = upload(client, manifest["id"], b"#EXTM3U\nhttp://127.0.0.1:9/private\nfile:///etc/passwd\n", filename="source." + extension)
                failed = command(client, manifest, "import", upload_ids=[upload_id])
                self.assertEqual(failed["command"]["state"], "failed", extension)
                self.assertEqual(failed["revision"], manifest["revision"])
            source.write_bytes(wav_bytes(1.2, rate=44100))
            encoded = folder / "long.mp3"
            subprocess.run([str(ffmpeg), "-v", "error", "-nostdin", "-i", str(source), str(encoded)], check=True, capture_output=True, timeout=30)
            with project_service(studio_cli=studio, ffmpeg=ffmpeg, v2_max_audio_seconds=1) as (_, bounded):
                project = create_project(bounded)
                upload_id = upload(bounded, project["id"], encoded.read_bytes(), filename="source.mp3")
                failed = command(bounded, project, "import", upload_ids=[upload_id])
                self.assertEqual((failed["command"]["state"], failed["revision"], failed["original_id"]), ("failed", 0, None))

    def test_suggest_loose_stem_labels_mix_replace_and_studio_bundle(self):
        studio, ffmpeg = native_studio(self)
        with project_service(studio_cli=studio, ffmpeg=ffmpeg) as (_, client):
            manifest = create_project(client)
            inputs = [upload(client, manifest["id"], wav_bytes(1.2), role="stem", label=label, kind=kind)
                      for label, kind in (("Lead Vocals", "vocals"), ("Rhythm", "drums"))]
            manifest = command(client, manifest, "import_stems", upload_ids=inputs)
            self.assertEqual(manifest["command"]["state"], "completed", manifest)
            self.assertEqual({stem["kind"] for stem in manifest["stems"]}, {"vocals", "drums"})
            stem_ids = {stem["id"] for stem in manifest["stems"]}
            self.assertNotIn(manifest["original_id"], stem_ids)
            self.assertNotIn(manifest["preview_id"], stem_ids)
            self.assertNotEqual(manifest["original_id"], manifest["preview_id"])
            self.assertTrue(all(stem["role"] == "stem" for stem in manifest["stems"]))
            before = client.request("GET", f"/v2/projects/{manifest['id']}/assets/{manifest['original_id']}/audio")[2]
            stream = io.BytesIO(before)
            stream.seek(12)
            while True:
                tag, size = struct.unpack("<4sI", stream.read(8))
                if tag == b"data":
                    values = array("f")
                    values.frombytes(stream.read(size))
                    break
                stream.seek(size + size % 2, 1)
            self.assertAlmostEqual(max(values), 2 * 6000 / 32768, places=6,
                                   msg="Before must contain the complete raw stem sum")
            controls = manifest["controls"]
            controls["tracks"][0]["gain_db"] = -3
            manifest = command(client, manifest, "mix", mix=controls)
            self.assertEqual(manifest["command"]["state"], "completed", manifest)
            mix_id = manifest["preview_id"]
            manifest = command(client, manifest, "suggest", audio_id=mix_id, settings={"profile": "neutral"})
            self.assertEqual(manifest["command"]["state"], "completed", manifest)
            self.assertEqual(len(manifest["command"]["result"]["bands"]), 7)
            stem_id = manifest["stems"][0]["id"]
            manifest = command(client, manifest, "replace", audio_id=stem_id)
            self.assertEqual(manifest["stems"][0]["id"], mix_id)
            manifest = command(client, manifest, "bundle", audio_id=mix_id, format="wav24", kind="studio", include_stems=True)
            self.assertEqual(manifest["command"]["state"], "completed", manifest)
            route = f"/v2/projects/{manifest['id']}/artifacts/{manifest['command']['artifact_id']}"
            with zipfile.ZipFile(io.BytesIO(client.request("GET", route)[2])) as archive:
                self.assertIn("stems/vocals.wav", archive.namelist())
                self.assertIn("stems/drums.wav", archive.namelist())
            original_id, stems = manifest["original_id"], manifest["stems"]
            preview_upload = upload(client, manifest["id"], wav_bytes(1.2))
            manifest = command(client, manifest, "import", upload_ids=[preview_upload], as_preview=True)
            self.assertEqual(manifest["command"]["state"], "completed", manifest["command"])
            self.assertEqual((manifest["original_id"], manifest["stems"]), (original_id, stems))
            self.assertNotEqual(manifest["preview_id"], mix_id)
            self.assertEqual(next(asset["role"] for asset in manifest["assets"] if asset["id"] == manifest["preview_id"]), "edited")
