"""Offline stdio discovery/consent/readiness regression; never downloads software."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import wave
from protocol_client import Client


EXPECTED_TOOLS = set("""get_capabilities get_help list_audio_files load_audio load_reference
clear_reference analyze_audio suggest_eq master_preview apply_eq get_session get_job_status
cancel_job open_in_desktop export_audio clear_session get_stem_engine_status install_stem_engine
get_studio_session analyze_studio_audio separate_stems import_stems inspect_stem_bundle
import_stem_bundle mix_stems clean_voice repair_audio undo_studio_edit redo_studio_edit
apply_preview_to_stem edit_stem_region scan_audio_issues preview_stem_doctor export_studio_audio export_stems save_studio_project open_studio_in_desktop
use_studio_for_mastering list_plugins probe_plugin render_plugin get_optional_tool_status
prepare_optional_tool prepare_support_report save_support_report""".split())


def verify(server):
    with tempfile.TemporaryDirectory(prefix="SlimeyBeats optional parity ") as temporary:
        root = Path(temporary)
        library = root / "Authorized audio"
        library.mkdir()
        cache = root / "Private user cache"
        cache.mkdir()
        source = library / "Voice.wav"
        with wave.open(str(source), "wb") as wav:
            wav.setparams((1, 2, 16000, 0, "NONE", ""))
            wav.writeframes(b"".join(struct.pack("<h", int(3000 * math.sin(i / 16000 * math.tau * 440))) for i in range(32000)))
        original = hashlib.sha256(source.read_bytes()).hexdigest()
        env = dict(os.environ, PATH="", LOCALAPPDATA=str(cache), XDG_CACHE_HOME=str(cache))
        private_binary = root / "Private companion" / server.name
        private_binary.parent.mkdir()
        shutil.copy2(server, private_binary)
        command = [str(private_binary), "--input-root", str(library), "--output-root", str(root / "Exports"),
                   "--studio-data", str(root / "Unprepared stem engine")]
        desktop = root / "Original GUI install" / "SlimeyBeats-EZ-Master.exe"
        desktop.parent.mkdir()
        desktop.write_bytes(b"Configuration fixture; never launched")
        config = json.loads(subprocess.check_output(command + ["--desktop-exe", str(desktop), "--print-config"], env=env, text=True))
        generated = config["mcpServers"]["slimeybeats"]
        assert os.path.samefile(generated["command"], private_binary)
        assert os.path.samefile(generated["args"][generated["args"].index("--desktop-exe") + 1], desktop)
        assert "--profiles-root" not in generated["args"], "Default configuration must remain bundled-only"
        client = Client(command, env=env)
        try:
            tools = {t["name"]: t for t in client.request("tools/list", {})["result"]["tools"]}
            assert set(tools) == EXPECTED_TOOLS, (set(tools) - EXPECTED_TOOLS, EXPECTED_TOOLS - set(tools))
            assert tools["get_optional_tool_status"]["annotations"]["readOnlyHint"]
            assert not tools["get_optional_tool_status"]["annotations"]["openWorldHint"]
            assert tools["prepare_optional_tool"]["annotations"]["openWorldHint"]
            schema = tools["prepare_optional_tool"]["inputSchema"]
            assert schema["additionalProperties"] is False
            assert schema["properties"]["allow_download"]["default"] is False
            tool_schema = schema["properties"]["tool"]
            if "$ref" in tool_schema:
                tool_schema = schema["$defs"][tool_schema["$ref"].split("/")[-1]]
            assert set(tool_schema["enum"]) == {"audio_codecs", "vst3_host"}
            caps = client.call("get_capabilities")
            policy = caps["optional_tools"]
            assert policy["allow_download_default"] is False
            assert policy["audio_codecs"]["max_download_bytes"] == {"windows_x86_64": 192 * 1024**2, "linux_x86_64_gnu": 160 * 1024**2}
            assert policy["vst3_host"]["max_download_bytes"] == 192 * 1024**2
            assert policy["vst3_host"]["preparation_mode"] == "explicit_private_recovery"
            assert policy["vst3_host"]["reuses_bundled_host"] is False
            assert "not total RAM" in policy["privacy"]
            quick = client.call("get_help")
            assert quick["optional_tools"]["steps"]
            assert client.call("get_help", {"topic": "optional_tools"})["steps"]
            states = client.call("get_optional_tool_status")["tools"]
            assert {state["tool"] for state in states} == {"audio_codecs", "vst3_host"}
            for state in states:
                assert state["component_version"] is None and state["installed_application_version"] is None
                if state["verification"] == "detected_only":
                    assert state["hashes_verified"] is None
            assert not next(s for s in states if s["tool"] == "audio_codecs")["available"]
            assert not next(s for s in states if s["tool"] == "vst3_host")["available"]
            loaded = client.run("load_audio", {"library": 0, "path": source.name})
            before = client.call("get_session")
            for tool in ("audio_codecs", "vst3_host"):
                for args in ({"tool": tool}, {"tool": tool, "allow_download": False}):
                    error = client.call("prepare_optional_tool", args, error=True)
                    assert "allow_download:true" in json.dumps(error)
                    assert client.call("get_session") == before
                client.call("prepare_optional_tool", {"tool": tool, "allow_download": True, "url": "https://untrusted.invalid/tool.zip"}, error=True)
                selected = client.call("get_optional_tool_status", {"tool": tool})
                assert len(selected["tools"]) == 1 and selected["tools"][0]["tool"] == tool
            client.call("prepare_optional_tool", {"tool": "arbitrary_plugin", "allow_download": True}, error=True)
            client.call("prepare_optional_tool", {"tool": "mcp_bridge", "allow_download": True}, error=True)
            assert client.call("get_session") == before
            # Ordinary native work must remain useful with no codecs/model, no PATH and no downloads.
            client.run("analyze_audio", {"source_id": loaded["source_id"]})
            voice = client.run("clean_voice", {"audio_id": loaded["source_id"], "settings": {"noise_reduction": 0, "deess": 0}})
            repair = client.run("repair_audio", {"audio_id": voice["studio_preview_id"], "selection": {
                "start_seconds": 0.5, "end_seconds": 1., "low_hz": 200., "high_hz": 1000., "reduction_db": -3., "mode": "attenuate"}})
            saved = client.run("export_studio_audio", {"studio_preview_id": repair["studio_preview_id"], "name": "Native WAV"})
            assert (root / "Exports" / saved["exports"][0]["path"]).is_file()
            failed = client.finish(client.call("separate_stems", {"source_id": loaded["source_id"]}))
            assert failed["status"] == "failed" and "install_stem_engine" in failed["error"]
            assert not (root / "Unprepared stem engine").exists()
            assert not list(cache.rglob("*")), "Discovery or unrelated native work wrote optional caches"
            refreshed = False
            if os.name == "nt":
                bundled = private_binary.parent / "slimeybeats-vst3-host.exe"
                bundled.write_bytes(b"Bundled detection fixture; deliberately never executed")
                detected = client.call("get_optional_tool_status", {"tool": "vst3_host"})["tools"][0]
                assert detected["available"] and detected["verification"] == "detected_only"
                assert detected["hashes_verified"] is None and detected["component_version"] is None
                bundled.unlink()
                # Model the verified-cache publication boundary without network or executing a helper.
                host = cache / "slimeybeats-ez-master/optional-tools/bundled/vst3_host"
                ready = host / "ready-fixture"
                ready.mkdir(parents=True)
                binary = b"Readiness fixture, deliberately never executed"
                name = "slimeybeats-vst3-host.exe"
                (ready / name).write_bytes(binary)
                (host / "owner.json").write_text("SlimeyBeats-EZ-Master:vst3_host:1", encoding="utf-8")
                (host / "current.json").write_text(json.dumps({"owner": "SlimeyBeats-EZ-Master", "tool": "vst3_host", "protocol": 1,
                    "directory": ready.name, "files": {name: hashlib.sha256(binary).hexdigest()}}), encoding="utf-8")
                state = client.call("get_optional_tool_status", {"tool": "vst3_host"})["tools"][0]
                assert state["available"] and state["hashes_verified"]
                assert state["verification"] == "private_executable_hash_verified" and state["component_version"] is None
                assert client.call("list_plugins")["host_available"]
                assert client.call("get_session")["connection_id"] == before["connection_id"]
                assert not client.call("get_capabilities")["studio"]["vst3_hosting"], "Preparation granted plugin-root access"
                session = client.call("get_session")
                bundled.write_bytes(b"Bundled detection fixture; deliberately never executed")
                prepared = client.run("prepare_optional_tool", {"tool": "vst3_host", "allow_download": True})
                assert prepared["tool"] == "vst3_host" and prepared["available"] and prepared["hashes_verified"]
                assert prepared["installed_application_updated"] is False and prepared["component_version"] is None
                assert prepared["preparation_scope"] == "private_recovery_cache"
                assert prepared["bundled_host_validated"] is False and prepared["bundled_host_replaced"] is False
                assert bundled.read_bytes() == b"Bundled detection fixture; deliberately never executed"
                assert client.call("get_optional_tool_status", {"tool": "vst3_host"})["tools"][0]["verification"] == "detected_only"
                bundled.unlink()
                assert client.call("get_session") == session, "Cached preparation changed audio/settings"
                refreshed = True
            assert hashlib.sha256(source.read_bytes()).hexdigest() == original
            return {"tools": len(tools), "explicit_consent": True, "native_work_without_setup": True,
                    "stem_setup_still_manual": True, "private_host_refresh_without_reconnect": refreshed,
                    "real_downloads": False, "source_preserved": True}
        finally:
            client.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.server.resolve()), indent=2))
