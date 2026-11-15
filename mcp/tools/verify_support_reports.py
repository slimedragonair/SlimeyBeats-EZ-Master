"""Offline real-stdio report privacy/review/non-overwrite qualification; synthetic audio only."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import tempfile
import wave

from protocol_client import Client


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(server):
    with tempfile.TemporaryDirectory(prefix="MCP support private fixtures ") as temporary:
        root = Path(temporary)
        library = root / "SecretUser Library"
        library.mkdir()
        output = root / "Authorized Output"
        cache = root / "Private Cache"
        cache.mkdir()
        profiles = root / "Private Profiles"
        profiles.mkdir()
        (profiles / "PrivateProfileName.json").write_text(json.dumps({
            "display_name": "PrivateProfileDisplay", "tonal_db": [0.] * 10,
            "lufs": {"streaming_safe": -16., "loud": -11.},
        }), encoding="utf-8")
        source = library / "UnreleasedSong.wav"
        with wave.open(str(source), "wb") as wav:
            wav.setparams((1, 2, 16000, 0, "NONE", ""))
            wav.writeframes(b"".join(struct.pack("<h", int(3200 * math.sin(i / 16000 * math.tau * 440)))
                                     for i in range(32000)))
        source_hash = digest(source)
        desktop = root / "Private GUI" / "SlimeyBeats-EZ-Master.exe"
        desktop.parent.mkdir()
        desktop.write_bytes(b"Configured GUI fixture; must never be queried or executed by reporting")
        env = dict(os.environ, PATH="", LOCALAPPDATA=str(cache), XDG_CACHE_HOME=str(cache),
                   PRIVATE_REPORT_CANARY="DoNotCollectEnvironment12345")
        command = [str(server), "--input-root", str(library), "--output-root", str(output),
                   "--profiles-root", str(profiles), "--desktop-exe", str(desktop),
                   "--studio-data", str(root / "Unused Stem Cache")]
        client = Client(command, env=env)
        try:
            tools = {tool["name"]: tool for tool in client.request("tools/list", {})["result"]["tools"]}
            assert len(tools) == 45, len(tools)
            for name in ("prepare_support_report", "save_support_report"):
                schema = tools[name]["inputSchema"]
                assert schema["additionalProperties"] is False
                assert tools[name]["annotations"]["openWorldHint"] is False
                for term in ("owner", "model provider", "preview"):
                    assert term in tools[name]["description"], (name, term)
            assert tools["prepare_support_report"]["annotations"]["readOnlyHint"]
            assert tools["save_support_report"]["inputSchema"]["properties"]["owner_review_confirmed"]["default"] is False
            caps = client.call("get_capabilities")
            assert caps["tool_count"] == 45
            assert caps["support_reports"]["installed_desktop_version"] is None
            assert caps["support_reports"]["in_memory_previews_per_connection"] == 1
            for topic in ("support_reports", "recovery"):
                assert client.call("get_help", {"topic": topic})

            loaded = client.run("load_audio", {"library": 0, "path": source.name})
            client.run("load_reference", {"library": 0, "path": source.name})
            master = client.run("master_preview", {"source_id": loaded["source_id"],
                                 "options": {"profile": "custom:PrivateProfileName", "intensity": 0}})
            preview_file = output / master["relative_preview_path"]
            preview_hash = digest(preview_file)
            before = client.call("get_session")
            before_files = {str(p.relative_to(output)) for p in output.rglob("*")}

            report = client.call("prepare_support_report", {
                "category": "audio_playback", "title": "Playback stops", "details": "Press Play, then Stop."
            })
            payload = report["preview_markdown"]
            assert len(payload.encode("utf-8")) == report["bytes"] <= 65536
            for secret in ("SecretUser", "UnreleasedSong", "PrivateProfileName", "PrivateProfileDisplay",
                           "DoNotCollectEnvironment12345", str(root), "Private GUI"):
                assert secret not in json.dumps(report), secret
            assert '"linked_engine_build"' in payload
            assert '"installed_desktop_version": null' in payload
            assert '"component": "mcp_companion"' in payload
            diagnostics = json.loads(payload.rsplit("```json\n", 1)[1].split("\n```", 1)[0])
            assert diagnostics["tools"] == dict.fromkeys(("audio_codecs_ready", "stem_engine_ready",
                                                          "vst3_host_ready", "mcp_bridge_ready"))
            assert "may reach its model provider" in payload
            assert client.call("get_session") == before
            assert before_files == {str(p.relative_to(output)) for p in output.rglob("*")}

            base_save = {"review_id": report["review_id"], "filename": "support-report.md"}
            client.call("save_support_report", base_save, error=True)
            client.call("save_support_report", dict(base_save, owner_review_confirmed=False), error=True)
            for filename in ("../escape.md", "/tmp/escape.md", "C:\\escape.md", "nested/report.md",
                             "CON.md", ".hidden.md", "report.txt", "report..md"):
                client.call("save_support_report", dict(base_save, filename=filename, owner_review_confirmed=True), error=True)
            client.call("save_support_report", dict(base_save, review_id="foreign-token", owner_review_confirmed=True), error=True)
            client.call("prepare_support_report", {"error": "No raw errors allowed"}, error=True)
            client.call("prepare_support_report", {"details": "x" * 16385}, error=True)
            client.call("save_support_report", dict(base_save, owner_review_confirmed=True, url="https://invalid.test"), error=True)

            saved = client.call("save_support_report", dict(base_save, owner_review_confirmed=True))
            assert saved["saved"] and saved["uploaded"] is False
            assert (output / "support-report.md").read_bytes() == payload.encode("utf-8")
            client.call("save_support_report", dict(base_save, owner_review_confirmed=True), error=True)
            assert (output / "support-report.md").read_bytes() == payload.encode("utf-8")
            client.call("save_support_report", dict(base_save, filename="support-report-copy.md", owner_review_confirmed=True))
            assert (output / "support-report-copy.md").read_bytes() == payload.encode("utf-8")

            replacement = client.call("prepare_support_report", {"details":
                "C:\\Users\\PrivatePerson\\SecretSong.wav\nprivate.person@example.test\nAPI_KEY=DO_NOT_SHARE"})
            assert replacement["review_id"] != report["review_id"]
            assert replacement["text_was_changed"]
            for secret in ("PrivatePerson", "SecretSong", "private.person", "DO_NOT_SHARE"):
                assert secret not in replacement["preview_markdown"]
            client.call("save_support_report", dict(base_save, filename="stale.md", owner_review_confirmed=True), error=True)
            assert not (output / "stale.md").exists()
            other = Client(command, env=env)
            try:
                other_report = other.call("prepare_support_report")
                assert other_report["review_id"] != replacement["review_id"]
                other.call("save_support_report", {"review_id": replacement["review_id"],
                           "filename": "foreign.md", "owner_review_confirmed": True}, error=True)
            finally:
                other.close()
            assert not (output / "foreign.md").exists()
            assert client.call("get_session") == before
            assert digest(preview_file) == preview_hash and digest(source) == source_hash
            assert not (root / "Unused Stem Cache").exists()
            return {"tools": len(tools), "schema_and_help": True, "explicit_owner_review_required": True,
                    "exact_preview_saved": True, "collision_and_stale_foreign_id_rejected": True,
                    "failed_prepare_preserved_previous_review": True, "privacy_canaries_absent": True,
                    "installed_desktop_version_unknown": True, "source_and_preview_hashes_unchanged": True,
                    "audio_reference_profile_settings_unchanged": True, "software_downloads": False,
                    "real_audio_used": False, "binary": str(server)}
        finally:
            client.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.server.resolve()), indent=2))
