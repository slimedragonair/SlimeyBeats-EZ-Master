"""Qualify GUI profile-root forwarding with a separately located cached companion."""
import argparse
import json
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import wave
from protocol_client import LegacyClient as Client


def verify(server):
    with tempfile.TemporaryDirectory(prefix="MCP profiles from original GUI ") as temporary:
        root = Path(temporary)
        private = root / "Private helper cache" / server.name
        private.parent.mkdir()
        shutil.copy2(server, private)
        profiles = root / "Original GUI install" / "profiles"
        profiles.mkdir(parents=True)
        profile = {"display_name": "Owner curve", "tonal_db": [0.] * 10,
                   "lufs": {"streaming_safe": -16., "loud": -11.}}
        (profiles / "Owner curve.json").write_text(json.dumps(profile), encoding="utf-8")
        desktop = profiles.parent / "SlimeyBeats-EZ-Master.exe"
        desktop.write_bytes(b"Original desktop configuration fixture; never launched")
        library = root / "Audio"
        library.mkdir()
        with wave.open(str(library / "Source.wav"), "wb") as wav:
            wav.setparams((1, 2, 16000, 0, "NONE", ""))
            wav.writeframes(b"".join(struct.pack("<h", int(3500 * math.sin(i / 16000 * math.tau * 440))) for i in range(32000)))
        command = [str(private), "--input-root", str(library), "--output-root", str(root / "Exports")]
        configured = json.loads(subprocess.check_output(command + ["--profiles-root", str(profiles), "--desktop-exe", str(desktop), "--print-config"], text=True))
        entry = configured["mcpServers"]["slimeybeats"]
        args = entry["args"]
        assert os.path.samefile(entry["command"], private)
        assert os.path.samefile(args[args.index("--profiles-root") + 1], profiles)
        assert os.path.samefile(args[args.index("--desktop-exe") + 1], desktop)
        client = Client([entry["command"], *args])
        try:
            caps = client.call("get_capabilities")
            assert caps["profile_catalog"]["custom_root_configured"]
            assert not caps["profile_catalog"]["warnings"]
            found = next(p for p in caps["profiles"] if p["id"] == "custom:Owner curve")
            assert found["targets_lufs"]["streaming_safe"] == -16. and found["source"] == "owner_configured"
            loaded = client.run("load_audio", {"library": 0, "path": "Source.wav"})
            master = client.run("master_preview", {"source_id": loaded["source_id"], "options": {"profile": "custom:Owner curve", "intensity": 0}})
            assert master["target_lufs"] == -16., master
            assert client.call("get_session")["settings"]["profile"] == "custom:Owner curve"
            assert client.run("analyze_audio", {"source_id": loaded["source_id"]})
            revised = client.run("apply_eq", {"preview_id": master["preview_id"], "options": {"output_gain_db": -1., "hold_loudness": False}})
            assert revised["target_lufs"] == -16.
            assert client.run("load_reference", {"library": 0, "path": "Source.wav"})["reference_loaded"]
            before = client.call("get_session")
            for profile_id in ("custom:../../outside", "../../outside", "custom:/private", "custom:Owner curve+neutral"):
                client.call("master_preview", {"source_id": loaded["source_id"], "options": {"profile": profile_id}}, error=True)
                assert client.call("get_session") == before
        finally:
            client.close()
        # Omitting the flag retains the original bundled-only API, even beside GUI custom files.
        client = Client(command)
        try:
            caps = client.call("get_capabilities")
            assert not caps["profile_catalog"]["custom_root_configured"]
            assert all(not p["id"].startswith("custom:") for p in caps["profiles"])
            loaded = client.run("load_audio", {"library": 0, "path": "Source.wav"})
            client.call("master_preview", {"source_id": loaded["source_id"], "options": {"profile": "custom:Owner curve"}}, error=True)
        finally:
            client.close()
        return {"profiles_root_forwarded": True, "private_companion_independent_of_GUI_root": True,
                "custom_master_analyze_eq_reference": True, "profile_paths_rejected": True,
                "default_still_bundled_only": True, "desktop_launched": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.server.resolve()), indent=2))
