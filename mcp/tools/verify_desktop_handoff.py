"""Verify a real MCP handoff and native GUI preparation in isolated folders."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
from protocol_client import Client


def verify(server, desktop, fixture, evidence):
    with tempfile.TemporaryDirectory(prefix="SlimeyBeats handoff with spaces ") as temporary:
        root = Path(temporary)
        library = root / "Input"
        library.mkdir()
        source = library / "My song.wav"
        content = fixture.read_bytes()
        source.write_bytes(content)
        output = root / "Exports"
        client = Client([str(server), "--input-root", str(library), "--output-root", str(output), "--desktop-exe", str(desktop)])
        process_id = None
        try:
            loaded = client.run("load_audio", {"library": 0, "path": source.name})
            mastered = client.run("master_preview", {"source_id": loaded["source_id"], "options": {"intensity": 0.6}})
            client.run("apply_eq", {"preview_id": mastered["preview_id"], "options": {"output_gain_db": -2.0, "hold_loudness": False}})
            source.write_bytes(content + b"changed")
            rejected = client.finish(client.call("open_in_desktop"))
            assert rejected["status"] == "failed" and "changed" in rejected["error"], rejected
            source.write_bytes(content)
            handoff = client.run("open_in_desktop")
            assert handoff["desktop_launched"], handoff
            process_id = handoff["process_id"]
            document = output / handoff["session_path"]
            saved = json.loads(document.read_text(encoding="utf-8"))
            assert saved["settings"]["intensity"] == 0.6
            assert saved["settings"]["eq_output_db"] == -2.0
            assert saved["source"]["identity"]["sha256"] == hashlib.sha256(content).hexdigest()
            # Stop only the process launched by this test, then reopen that same
            # document using the GUI's existing self-closing diagnostic capture.
            time.sleep(0.5)
            os.kill(process_id, signal.SIGTERM)
            process_id = None
            capture = evidence / "desktop-handoff.png"
            result = subprocess.run([str(desktop), "--session", str(document), "--auto-master", "--view", "eq", "--capture-ui", str(capture)],
                                    capture_output=True, text=True, timeout=90)
            assert result.returncode == 0, result.stderr
            state = json.loads(capture.with_suffix(".state.json").read_text(encoding="utf-8"))
            assert state["error"] is None and state["preview_ready"], state
            assert not list(output.glob("*.wav")), "Handoff exported audio"
            assert source.read_bytes() == content
            return {"changed_source_rejected": True, "settings_preserved": True,
                    "trusted_desktop_launched": True, "gui_state": state, "no_auto_export": True}
        finally:
            if process_id is not None:
                os.kill(process_id, signal.SIGTERM)
            client.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--desktop", type=Path, required=True)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    args.evidence.mkdir(parents=True, exist_ok=True)
    print(json.dumps(verify(args.server.resolve(), args.desktop.resolve(), args.input, args.evidence.resolve()), indent=2))
