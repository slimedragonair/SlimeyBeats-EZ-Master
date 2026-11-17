"""Real stdio/disk/restart proof in a retained private fixture; no physical playback."""
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
import time
import wave
from protocol_client import Client


class RecordedClient(Client):
    def __init__(self, command, transcript):
        self.transcript = transcript
        super().__init__(command)

    def request(self, method, params):
        response = super().request(method, params)
        self.transcript.append({"method": method, "params": params, "response": response})
        return response


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def revision(state):
    return {"session_id": state["session_id"], "expected_revision": state["revision"]}


def replacement(state, project_id):
    return {"project_id": project_id, "replace_session_id": state["session_id"],
            "expected_revision": state["revision"]}


def tree_hashes(root):
    return {str(p.relative_to(root)): digest(p) for p in root.rglob("*") if p.is_file()}


def tone(path, seconds, frequency):
    rate = 48000
    with wave.open(str(path), "wb") as wav:
        wav.setparams((2, 2, rate, 0, "NONE", "not compressed"))
        frames = bytearray()
        for frame in range(round(rate * seconds)):
            value = round(2200 * math.sin(2 * math.pi * frequency * frame / rate)
                          + 800 * math.sin(2 * math.pi * 12000 * frame / rate))
            frames.extend(struct.pack("<hh", value, round(value * 0.8)))
        wav.writeframes(frames)


def rejection(client, name, arguments):
    response = client.request("tools/call", {"name": name, "arguments": arguments})
    if "error" in response or response.get("result", {}).get("isError"):
        return response
    result = response["result"]["structuredContent"]
    job = client.finish(result)
    assert job["status"] == "failed", job
    return job


def verify(server, evidence, probe_source):
    evidence.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="MCP arrangement projects ", dir=evidence)).resolve()
    binary = root / server.name
    shutil.copy2(server, binary)
    probe = root / ("desktop-probe.exe" if os.name == "nt" else "desktop-probe")
    subprocess.run(["rustc", "--edition=2024", str(probe_source), "-o", str(probe)], check=True,
                   capture_output=True, text=True)
    launches = probe.with_suffix(".launches.jsonl")
    capability_probes = probe.with_suffix(".capability-probes.jsonl")
    capability_mode = probe.with_suffix(".mode")
    library, output = root / "Music", root / "Exports"
    library.mkdir()
    sources = {"Voice.wav": (4, 440), "Backing.wav": (6, 220),
               "Legacy voice.wav": (1, 330), "Legacy backing.wav": (1, 110)}
    for name, (seconds, frequency) in sources.items():
        tone(library / name, seconds, frequency)
    hashes = {name: digest(library / name) for name in sources}
    transcript, checks = [], {}
    report = {"root": str(root), "server_sha256": digest(binary), "source_hashes_before": hashes,
              "physical_playback": False, "desktop_probe": "Argument/header recorder, not GUI acceptance"}
    clients = []

    def start(out=output, mib=16):
        client = RecordedClient([str(binary), "--input-root", str(library), "--output-root", str(out),
                                 "--max-decoded-mib", str(mib), "--desktop-exe", str(probe)], transcript)
        clients.append(client)
        return client

    def close(client):
        client.close()
        clients.remove(client)

    try:
        client = start()
        tools = client.request("tools/list", {})["result"]["tools"]
        caps = client.call("get_capabilities")
        assert len(tools) == caps["tool_count"] == 58
        assert {"save_arrangement_project", "load_arrangement_project", "open_arrangement_in_desktop"} <= {t["name"] for t in tools}
        descriptions = {tool["name"]: tool["description"] for tool in tools}
        for legacy, current in [("save_studio_project", "save_arrangement_project"),
                                ("open_studio_in_desktop", "open_arrangement_in_desktop"),
                                ("export_studio_audio", "export_arrangement"),
                                ("undo_studio_edit", "undo_arrangement"),
                                ("redo_studio_edit", "undo_arrangement")]:
            assert "legacy" in descriptions[legacy].lower() and current in descriptions[legacy], legacy
        legacy_help = client.call("get_help", {"topic": "studio_export"})
        assert "LEGACY" in legacy_help["scope"] and "both workspaces exist" in legacy_help["scope"]
        checks["legacy_tool_descriptions_and_export_help_explicitly_route_tracks_to_arrangement_tools"] = True
        help_text = client.call("get_help", {"topic": "arrangement"})["persistence"]
        assert all(key in help_text for key in ["save", "restart_reopen", "replace_open_workspace", "desktop"])
        state = client.call("create_arrangement", {"sample_rate": 48000, "channels": 2})
        for name in ["Voice.wav", "Backing.wav"]:
            source = client.run("load_audio", {"library": 0, "path": name})
            state = client.call("add_arrangement_assets", {**revision(state), "sources": [
                {"audio_id": source["source_id"], "name": name[:-4]}]})
        clip = state["arrangement"]["lanes"][0]["clips"][0]["id"]
        lane = state["arrangement"]["lanes"][0]["id"]
        bands = state["defaults"]["bands"]
        bands[-1]["gain"] = -6
        state = client.call("edit_arrangement", {**revision(state), "edits": [
            {"kind": "move_clip", "clip_id": clip, "lane_id": lane, "start_frame": 96000},
            {"kind": "set_eq", "effect": {"id": "voice-section", "scope": {"kind": "clip", "clip_id": clip},
             "range": {"start_frame": 48000, "end_frame": 96000}, "bands": bands}}]})
        state = client.call("edit_arrangement", {**revision(state), "edits": [
            {"kind": "set_lane", "lane_id": lane, "gain_db": -2}]})
        state = client.call("undo_arrangement", revision(state))
        expected_document, expected_history = state["arrangement"], state["history"]
        source_id = client.call("get_session")["source"]["source_id"]
        client.run("master_preview", {"source_id": source_id})
        client.run("import_stems", {"files": [
            {"library": 0, "path": "Legacy voice.wav", "kind": "vocals"},
            {"library": 0, "path": "Legacy backing.wav", "kind": "instrumental"}]})
        legacy = client.call("get_session")
        assert not capability_probes.exists(), "Startup/discovery/audio tools eagerly probed the desktop"
        assert not output.joinpath(".slimeybeats-arrangement-projects").exists()
        version_label = subprocess.check_output([str(probe), "--version"], text=True)
        assert "0.5.0-beta.3" in version_label and "1501" in version_label
        capability_rejections = {}
        for mode in ["old-same-version", "incomplete", "malformed", "timeout", "oversized"]:
            capability_mode.write_text(mode, encoding="utf-8")
            before = sorted(str(path.relative_to(output)) for path in output.rglob("*"))
            started = time.monotonic()
            failed = rejection(client, "open_arrangement_in_desktop", revision(state))
            elapsed = time.monotonic() - started
            assert "matching preview" in str(failed) and "capability verification failed" in str(failed), failed
            assert elapsed < 8, (mode, elapsed)
            assert sorted(str(path.relative_to(output)) for path in output.rglob("*")) == before
            assert not output.joinpath(".slimeybeats-arrangement-projects").exists()
            assert not launches.exists(), "Rejected capability probe handed off a project"
            assert client.call("get_arrangement") == state
            assert client.call("get_session") == legacy
            capability_rejections[mode] = {"elapsed_seconds": elapsed, "failure": failed}
        capability_mode.write_text("", encoding="utf-8")
        checks["lazy_bounded_capability_probe_rejects_same_version_old_incomplete_malformed_timeout_and_oversized_before_any_project_folder"] = True
        report["desktop_capability_rejections"] = capability_rejections
        first = client.run("save_arrangement_project", revision(state))
        second = client.run("save_arrangement_project", revision(state))
        assert len(capability_probes.read_text(encoding="utf-8").splitlines()) == 5, "Save-only operation probed the desktop"
        assert first["project_id"] != second["project_id"]
        assert client.call("get_session") == legacy
        assert client.call("get_arrangement") == state
        assert not launches.exists(), "Discovery/save launched the desktop"
        assert not list(output.glob("*.wav")), "Project save exported a final mix"
        checks["58_tools_help_and_nonclobber_v2_save_preserve_master_legacy_and_arrangement"] = True
        for args in [{"project_id": first["project_id"]},
                     {**replacement(state, first["project_id"]), "expected_revision": 0},
                     {**replacement(state, first["project_id"]), "path": "../escape"}]:
            rejection(client, "load_arrangement_project", args)
            assert client.call("get_arrangement") == state
        rejection(client, "save_arrangement_project", {**revision(state), "expected_revision": 0})
        rejection(client, "open_arrangement_in_desktop", {**revision(state), "desktop_exe": "untrusted.exe"})
        desktop = client.run("open_arrangement_in_desktop", revision(state))
        assert desktop["desktop_launched"]
        assert desktop["desktop_capabilities"]["studio_project_versions"] == [1, 2]
        assert desktop["desktop_capabilities"]["arrangement_live_editing"] is True
        assert desktop["desktop_capabilities"]["autoplay_on_project_open"] is False
        assert len(capability_probes.read_text(encoding="utf-8").splitlines()) == 6
        deadline = time.monotonic() + 5
        while not launches.exists() and time.monotonic() < deadline:
            time.sleep(0.02)
        recorded = [json.loads(line) for line in launches.read_text(encoding="utf-8").splitlines()]
        assert len(recorded) == 1 and recorded[0]["flag"] == "--studio-session"
        assert Path(recorded[0]["manifest"]).samefile(output / desktop["project_path"])
        assert recorded[0]["canonical_v2_header"] is True
        checks["only_explicit_desktop_tool_launched_configured_executable_with_v2_project"] = True
        report["desktop_handoff"] = desktop
        close(client)

        originals = root / "Retained originals"
        originals.mkdir()
        for name in sources:
            (library / name).rename(originals / name)
        client = start()
        empty_legacy = client.call("get_session")
        disk_before = tree_hashes(output)
        restored = client.run("load_arrangement_project", {"project_id": first["project_id"]})
        assert restored["session_id"] != state["session_id"]
        assert restored["arrangement"] == expected_document and restored["history"] == expected_history
        assert restored["preview"] is None
        assert client.call("get_session") == empty_legacy
        assert tree_hashes(output) == disk_before
        assert not list(output.glob(".slimeybeats-mcp-*"))
        assert len(launches.read_text(encoding="utf-8").splitlines()) == 1
        assert len(capability_probes.read_text(encoding="utf-8").splitlines()) == 6, "Headless reopen probed the desktop"
        state = client.call("get_arrangement")
        redone = client.call("undo_arrangement", {**revision(state), "redo": True})
        assert redone["arrangement"]["lanes"][0]["gain_db"] == -2
        state = client.call("undo_arrangement", revision(redone))
        assert state["arrangement"] == expected_document
        checks["restart_reopen_without_library_files_restores_metadata_eq_undo_redo_without_render_launch_or_disk_write"] = True

        for name in ["../escape", "C:/foreign/project.json", "arrangement-foreign000"]:
            rejection(client, "load_arrangement_project", replacement(state, name))
            assert client.call("get_arrangement") == state
        foreign = start(root / "Foreign exports")
        rejection(foreign, "load_arrangement_project", {"project_id": first["project_id"]})
        assert foreign.call("get_arrangement")["arrangement"] is None
        close(foreign)
        checks["foreign_path_and_stale_rejections_preserve_prior_workspace"] = True

        broken_manifest = output / second["project_path"]
        saved_manifest = json.loads(broken_manifest.read_text(encoding="utf-8"))
        asset = broken_manifest.parent / saved_manifest["sources"][0]["file"]
        data = bytearray(asset.read_bytes())
        data[-1] ^= 1
        asset.write_bytes(data)
        rejection(client, "load_arrangement_project", replacement(state, second["project_id"]))
        assert client.call("get_arrangement") == state
        broken_manifest.write_text("{corrupt", encoding="utf-8")
        rejection(client, "load_arrangement_project", replacement(state, second["project_id"]))
        assert client.call("get_arrangement") == state
        checks["asset_and_manifest_corruption_preserve_prior_workspace"] = True

        linked_assets = (output / desktop["project_path"]).parent / "assets"
        retained_assets = linked_assets.with_name("assets-retained")
        linked_assets.rename(retained_assets)
        if os.name == "nt":
            created = subprocess.run(["cmd", "/c", "mklink", "/J", str(linked_assets), str(retained_assets)],
                                     capture_output=True, text=True, creationflags=subprocess.CREATE_NO_WINDOW)
            assert created.returncode == 0, created.stderr
            assert os.path.isjunction(linked_assets)
        else:
            linked_assets.symlink_to(retained_assets, target_is_directory=True)
        try:
            rejected = rejection(client, "load_arrangement_project", replacement(state, desktop["project_id"]))
            assert "reparse" in str(rejected) or "symlink" in str(rejected)
            assert client.call("get_arrangement") == state
        finally:
            assert linked_assets.parent.resolve().is_relative_to(root)
            if os.name == "nt":
                assert os.path.isjunction(linked_assets)
                os.rmdir(linked_assets)
            else:
                linked_assets.unlink()
            retained_assets.rename(linked_assets)
        checks["in_root_asset_junction_or_symlink_is_rejected"] = True

        large = library / "Large.wav"
        with wave.open(str(large), "wb") as wav:
            wav.setparams((2, 2, 48000, 0, "NONE", "not compressed"))
            wav.writeframes(struct.pack("<hh", 1234, 987) * 1180000)
        state = client.call("create_arrangement", {"replace_session_id": state["session_id"], "sample_rate": 48000, "channels": 2})
        source = client.run("load_audio", {"library": 0, "path": large.name})
        state = client.call("add_arrangement_assets", {**revision(state), "sources": [{"audio_id": source["source_id"]}]})
        big = client.run("save_arrangement_project", revision(state))
        close(client)
        client = start(mib=8)
        failed = rejection(client, "load_arrangement_project", {"project_id": big["project_id"]})
        assert "budget" in str(failed) and client.call("get_arrangement")["arrangement"] is None
        checks["restart_under_smaller_configured_source_budget_rejects_prior_valid_large_project"] = True
        close(client)
        report["source_hashes_after"] = {name: digest(originals / name) for name in sources}
        assert report["source_hashes_after"] == hashes
        checks["original_file_hashes_unchanged"] = True
        report.update({"checks": checks, "passed": all(checks.values()), "project_for_native_gui": str(output / first["project_path"]),
                       "known_good_project_id": first["project_id"], "desktop_probe_argv": recorded,
                       "limits": ["The configured desktop was a disposable argument recorder. Actual native GUI reopening remains a separate acceptance check.",
                                  "Cancellation and late-stale adoption are verified by deterministic Rust worker-barrier tests, not timing-dependent stdio cancellation."]})
    except Exception as error:
        report.update({"passed": False, "error": repr(error), "checks": checks})
        raise
    finally:
        for client in list(clients):
            try:
                close(client)
            except Exception as error:
                report.setdefault("close_errors", []).append(repr(error))
        report["transcript"] = transcript
        (root / "results.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps({"results": str(root / "results.json"), "passed": report.get("passed"), "checks": checks,
                          "project_for_native_gui": report.get("project_for_native_gui")}, indent=2), flush=True)
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    verify(args.server.resolve(), args.evidence.resolve(), Path(__file__).parent / "fixtures" / "arrangement_desktop_probe.rs")
