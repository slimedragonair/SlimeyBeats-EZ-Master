"""Exercise Studio via actual stdio; neural checks are optional and use an installed cache."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import tempfile
import wave
from protocol_client import Client


def verify(server, engine):
    with tempfile.TemporaryDirectory(prefix="SlimeyBeats Studio ") as temporary:
        root = Path(temporary)
        library = root / "Music"
        library.mkdir()
        source = library / "Test voice.wav"
        with wave.open(str(source), "wb") as wav:
            wav.setparams((2, 2, 16000, 0, "NONE", ""))
            samples = bytearray()
            for i in range(64000):
                t = i / 16000
                value = int(5000 * math.sin(t * math.tau * 750) + 1500 * math.sin(t * math.tau * 3000))
                samples.extend(struct.pack("<hh", value, value))
            wav.writeframes(samples)
        original = hashlib.sha256(source.read_bytes()).hexdigest()
        output = root / "Exports"
        command = [str(server), "--input-root", str(library), "--output-root", str(output)]
        if engine:
            command += ["--studio-data", str(engine)]
        client = Client(command)
        try:
            names = {tool["name"] for tool in client.request("tools/list", {})["result"]["tools"]}
            required = {"clean_voice", "repair_audio", "separate_stems", "mix_stems", "get_studio_session",
                        "undo_studio_edit", "redo_studio_edit", "export_studio_audio", "get_stem_engine_status"}
            assert required <= names
            for topic in ("stems", "voice", "repair", "studio_export"):
                assert client.call("get_help", {"topic": topic})
            loaded = client.run("load_audio", {"library": 0, "path": source.name})
            source_id = loaded["source_id"]
            cleaned = client.run("clean_voice", {"audio_id": source_id, "settings": {"noise_reduction": 0, "deess": 0}})
            first = cleaned["studio_preview_id"]
            assert not list(output.glob("*.wav")), "Voice cleanup exported automatically"
            assert client.call("get_studio_session")["can_undo"]
            repaired = client.run("repair_audio", {"audio_id": first, "selection": {
                "start_seconds": 1.0, "end_seconds": 2.0, "low_hz": 2500.0,
                "high_hz": 3500.0, "reduction_db": -12.0, "mode": "attenuate"}})
            second = repaired["studio_preview_id"]
            assert client.run("undo_studio_edit")["preview"]["studio_preview_id"] == first
            assert client.run("redo_studio_edit")["preview"]["studio_preview_id"] == second
            client.call("export_studio_audio", {"studio_preview_id": first, "name": "stale"}, error=True)
            saved = client.run("export_studio_audio", {"studio_preview_id": second, "name": "Edited voice"})
            assert (output / saved["exports"][0]["path"]).is_file()
            report = json.loads((output / saved["report_path"]).read_text(encoding="utf-8"))
            assert report["kind"] == "studio_audio" and len(report["steps"]) == 2
            assert client.call("get_session")["master"] is None
            client.run("undo_studio_edit")
            baseline = client.run("undo_studio_edit")
            assert baseline["preview"]["studio_preview_id"] == source_id
            assert not baseline["can_undo"]
            checked = ["tool discovery/help", "voice preview", "localized repair", "first-edit undo",
                       "redo", "stale export rejection", "explicit export/report", "master isolation"]
            if engine:
                assert client.call("get_stem_engine_status")["ready"]
                for mode, kinds in (("vocal_instrumental", {"vocals", "instrumental"}),
                                    ("four_stems", {"vocals", "drums", "bass", "other"})):
                    separated = client.run("separate_stems", {"source_id": source_id, "mode": mode})
                    bank = separated["stem_set"]
                    assert {control["kind"] for control in bank["controls"]["tracks"]} == kinds
                    mixed = client.run("mix_stems", {"stem_set_id": bank["stem_set_id"],
                        "options": {"tracks": [{"kind": "vocals", "gain_db": -3.0, "solo": True}]}})
                    assert mixed["studio_preview_id"]
                    restored = client.run("undo_studio_edit")
                    vocal = next(c for c in restored["stem_set"]["controls"]["tracks"] if c["kind"] == "vocals")
                    assert vocal["gain_db"] == 0 and not vocal["solo"]
                    checked.append(mode + " real model/mix/undo")
            assert hashlib.sha256(source.read_bytes()).hexdigest() == original
            return {"tools": len(names), "passed": checked, "source_preserved": True}
        finally:
            client.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--engine", type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.server.resolve(), args.engine.resolve() if args.engine else None), indent=2))
