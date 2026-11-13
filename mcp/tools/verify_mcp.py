"""Exercise the actual MCP wire protocol, DSP, scoping and non-clobber exports."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import wave
from protocol_client import Client

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "rust/tools"))
from verify_exports import independent_measurements


def verify(server, fixture, codecs):
    with tempfile.TemporaryDirectory(prefix="SlimeyBeats MCP with spaces ") as temporary:
        root = Path(temporary)
        library = root / "Audio library"
        output = root / "New exports"
        library.mkdir()
        source = library / "Source song.wav"
        if fixture:
            shutil.copy2(fixture, source)
        else:
            with wave.open(str(source), "wb") as wav:
                wav.setparams((2, 2, 16000, 0, "NONE", "not compressed"))
                frames = bytearray()
                for i in range(48000):
                    t = i / 16000
                    value = int(3500 * math.sin(t * 2 * math.pi * 220) + 1200 * math.sin(t * 2 * math.pi * 1700))
                    frames.extend(struct.pack("<hh", value, int(value * 0.8)))
                wav.writeframes(frames)
        original_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        (root / "outside.wav").write_bytes(b"outside")
        (library / "broken.wav").write_bytes(b"not a WAV")
        command = [str(server), "--input-root", str(library), "--output-root", str(output)]
        configuration = json.loads(subprocess.check_output(command + ["--print-config"], text=True))
        assert configuration["mcpServers"]["slimeybeats"]["command"]
        client = Client(command)
        try:
            tools = client.request("tools/list", {})["result"]["tools"]
            assert len(tools) == 16, len(tools)
            assert all("inputSchema" in tool and "annotations" in tool for tool in tools)
            caps = client.call("get_capabilities")
            assert caps["concurrent_audio_jobs"] == 1
            neutral = next(p for p in caps["profiles"] if p["id"] == "neutral")
            assert isinstance(neutral["targets_lufs"]["streaming_safe"], (int, float))
            guide = client.call("get_help")
            assert guide["steps"] and guide["job_example"]
            for topic in ("mastering", "eq", "export", "troubleshooting", "privacy"):
                assert client.call("get_help", {"topic": topic})
            # Some hosts consume text content instead of structuredContent.
            raw = client.request("tools/call", {"name": "get_help", "arguments": {}})["result"]
            assert json.loads(raw["content"][0]["text"]) == raw["structuredContent"]
            files = client.call("list_audio_files", {"library": 0})["files"]
            assert len(files) == 2
            nested = library / "Album"
            nested.mkdir()
            (nested / "Nested.wav").write_bytes(b"placeholder")
            listing = client.call("list_audio_files", {"library": 0})
            assert "Album" in listing["folders"]
            assert client.call("list_audio_files", {"library": 0, "folder": "Album"})["files"][0]["path"]
            client.call("load_audio", {"library": 0, "path": "../outside.wav"}, error=True)
            loaded = client.run("load_audio", {"library": 0, "path": source.name})
            source_id = loaded["source_id"]
            failed = client.finish(client.call("load_audio", {"library": 0, "path": "broken.wav"}))
            assert failed["status"] == "failed"
            assert client.call("get_session")["source"]["source_id"] == source_id
            client.call("master_preview", {"source_id": source_id, "options": {"profile": "../../private"}}, error=True)
            client.call("master_preview", {"source_id": source_id, "options": {"out_dir": "outside"}}, error=True)
            client.call("analyze_audio", {"source_id": "stale"}, error=True)
            measured = client.run("analyze_audio", {"source_id": source_id})
            assert measured["duration_seconds"] > 2
            assert "severity" in measured["interpretation"]
            for valid, deviation in zip(measured["valid_bands"], measured["deviation_db"]):
                if not valid:
                    assert deviation is None
            assert all(issue["active"] == (issue["severity"] > 0) for issue in measured["issues"])
            master = client.run("master_preview", {"source_id": source_id})
            first_path = output / master["relative_preview_path"]
            assert first_path.is_file()
            assert master["preview_bytes"] == first_path.stat().st_size > 44
            assert "No desktop executable is configured" in master["listening"]
            assert not list(output.glob("*.wav")), "Mastering exported final audio"
            for _ in range(2):
                proposal = client.run("suggest_eq", {"source_id": source_id})
                if "previous_proposal" in locals():
                    assert proposal == previous_proposal
                previous_proposal = proposal
            original_preview = master["preview_id"]
            master = client.run("apply_eq", {"preview_id": original_preview, "options": {"output_gain_db": -1.5, "hold_loudness": False}})
            assert not first_path.exists(), "Replaced preview leaked"
            eq_hash = hashlib.sha256((output / master["relative_preview_path"]).read_bytes()).hexdigest()
            master = client.run("apply_eq", {"preview_id": master["preview_id"], "options": {"output_gain_db": -1.5, "hold_loudness": False}})
            assert eq_hash == hashlib.sha256((output / master["relative_preview_path"]).read_bytes()).hexdigest()
            client.call("export_audio", {"preview_id": original_preview, "name": "stale"}, error=True)
            client.call("export_audio", {"preview_id": master["preview_id"], "name": "../escape"}, error=True)
            formats = ["wav16", "wav24", "wav32f"] + (["flac16", "flac24", "mp3"] if codecs else [])
            exports = []
            for format_name in formats:
                result = client.run("export_audio", {"preview_id": master["preview_id"], "name": "Test master", "format": format_name})
                assert result["warning"] is None
                item = result["exports"][0]
                path = output / item["path"]
                assert path.is_file()
                assert item["decoded_metrics"]["true_peak_dbtp"] <= -0.995
                if codecs:
                    lufs, peak = independent_measurements(path)
                    assert peak <= -0.95, (format_name, peak)
                    assert abs(lufs - item["decoded_metrics"]["integrated_lufs"]) <= 0.15
                report = json.loads((output / result["report_path"]).read_text(encoding="utf-8"))
                assert "out_dir" not in report["configuration"] and "reference" not in report["configuration"]
                exports.append(path)
            saved_hash = hashlib.sha256(exports[1].read_bytes()).hexdigest()
            duplicate = client.run("export_audio", {"preview_id": master["preview_id"], "name": "Test master", "format": "wav24"})
            assert output / duplicate["exports"][0]["path"] != exports[1]
            assert hashlib.sha256(exports[1].read_bytes()).hexdigest() == saved_hash
            if codecs:
                mp3 = library / "Compressed song.mp3"
                subprocess.run(["ffmpeg", "-v", "error", "-i", str(source), "-c:a", "libmp3lame", str(mp3)], check=True)
                client.run("load_audio", {"library": 0, "path": mp3.name})
                loaded = client.run("load_audio", {"library": 0, "path": source.name})
                source_id = loaded["source_id"]
            client.run("load_reference", {"library": 0, "path": source.name})
            assert client.call("get_session")["master"] is None
            client.run("clear_reference")
            assert not client.call("get_session")["reference_loaded"]
            client.call("clear_session")
            assert client.call("get_session")["source"] is None
            assert hashlib.sha256(source.read_bytes()).hexdigest() == original_hash
            assert all(path.is_file() for path in exports)
            return {"tools": len(tools), "formats": formats, "source_seconds": loaded["duration_seconds"],
                    "passed": ["initialize/schema", "folder scope", "failed import preservation", "stale IDs", "option validation", "analysis", "preview before export", "deterministic suggestions", "non-cumulative EQ", "preview cleanup", "non-clobber exports", "decoded peak checks", "report privacy", "reference invalidation", "source integrity", "config generation", "clean shutdown"]}
        finally:
            client.close()
            assert not list(output.glob(".slimeybeats-mcp-*")), "Temporary preview cache survived clean shutdown"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--input", type=Path)
    parser.add_argument("--codecs", action="store_true")
    args = parser.parse_args()
    print(json.dumps(verify(args.server.resolve(), args.input, args.codecs), indent=2))
