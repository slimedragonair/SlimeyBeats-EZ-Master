"""Exercise compact discovery, typed calls and legacy compatibility over real stdio."""
import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import re
import shutil
import struct
import tempfile
import wave
from protocol_client import Client


RETAINED = set("""get_capabilities get_help list_audio_files load_audio get_session
get_job_status cancel_job suggest_eq master_preview apply_eq clear_session
separate_stems mix_stems edit_stem_region apply_preview_to_stem scan_audio_issues
use_studio_for_mastering suggest_arrangement_eq render_arrangement""".split())
GROUPS = {
    "reference": {"load", "clear"},
    "analyze": {"mastering", "studio"},
    "export_result": {"mastering", "studio", "arrangement", "raw_stems"},
    "open_desktop": {"mastering", "studio", "arrangement"},
    "stem_import": {"inspect_zip", "zip", "files"},
    "process_audio": {"voice_cleanup", "spectral_repair", "stem_doctor"},
    "studio_history": {"undo", "redo"},
    "arrangement_edit": {"create", "add_assets", "apply", "undo", "redo"},
    "arrangement_inspect": {"state", "waveform", "eq_response"},
    "project": {"save_arrangement", "open_arrangement", "save_studio"},
    "optional_tool_status": set(),
    "setup_optional_tool": {"component", "stem_engine"},
    "plugin": {"list", "probe", "render"},
    "support_report": {"preview", "save"},
}
LEGACY = RETAINED | set("""load_reference clear_reference analyze_audio
analyze_studio_audio get_studio_session export_audio export_studio_audio
export_arrangement export_stems open_in_desktop open_studio_in_desktop
open_arrangement_in_desktop inspect_stem_bundle import_stem_bundle import_stems
clean_voice repair_audio preview_stem_doctor undo_studio_edit redo_studio_edit
create_arrangement add_arrangement_assets edit_arrangement undo_arrangement
get_arrangement get_arrangement_waveform get_arrangement_eq_response
save_arrangement_project load_arrangement_project save_studio_project
get_optional_tool_status get_stem_engine_status prepare_optional_tool
install_stem_engine list_plugins probe_plugin render_plugin
prepare_support_report save_support_report""".split())
COMPACT = RETAINED | set(GROUPS)


class RecordedClient(Client):
    def __init__(self, command, transcript, profile):
        self.transcript, self.profile = transcript, profile
        super().__init__(command)

    def request(self, method, params):
        response = super().request(method, params)
        self.transcript.append({"profile": self.profile, "method": method,
                                "params": params, "response": response})
        return response


def operation(action, **fields):
    return {"operation": {"action": action, **fields}}


def revision(state):
    return {"session_id": state["session_id"], "expected_revision": state["revision"]}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def files(root):
    return {str(p.relative_to(root)): digest(p) for p in root.rglob("*") if p.is_file()}


def resolve(schema, root):
    while "$ref" in schema:
        reference = schema["$ref"]
        assert reference.startswith("#/"), f"Non-local schema reference: {reference}"
        schema = root
        for part in reference[2:].split("/"):
            schema = schema[part.replace("~1", "/").replace("~0", "~")]
    return schema


def assert_typed(schema, root, path, seen=None):
    """Reject unconstrained JSON payloads while allowing typed maps and null branches."""
    seen = set() if seen is None else seen
    if schema is False:
        return
    assert isinstance(schema, dict) and schema, f"Unconstrained JSON schema at {path}"
    if "$ref" in schema:
        if schema["$ref"] in seen:
            return
        seen.add(schema["$ref"])
        schema = resolve(schema, root)
    assert any(key in schema for key in ("type", "oneOf", "anyOf", "allOf", "enum", "const")), path
    types = schema.get("type", [])
    if types == "object" or isinstance(types, list) and "object" in types:
        assert "properties" in schema or isinstance(schema.get("additionalProperties"), dict), f"Untyped object payload at {path}"
    for key in ("oneOf", "anyOf", "allOf"):
        for index, branch in enumerate(schema.get(key, [])):
            assert_typed(branch, root, f"{path}.{key}[{index}]", seen)
    for name, field in schema.get("properties", {}).items():
        assert_typed(field, root, f"{path}.{name}", seen)
    if "items" in schema:
        assert_typed(schema["items"], root, f"{path}[]", seen)
    if "additionalProperties" in schema and schema["additionalProperties"] is not False:
        assert_typed(schema["additionalProperties"], root, f"{path}.*", seen)


def catalog(client, profile):
    tools = client.request("tools/list", {})["result"]["tools"]
    names = [tool["name"] for tool in tools]
    expected = COMPACT if profile == "compact" else LEGACY
    assert len(names) == len(set(names)) == len(expected), (profile, len(names))
    assert set(names) == expected, {"missing": sorted(expected - set(names)), "unexpected": sorted(set(names) - expected)}
    if profile == "compact":
        assert len(names) == 33 and len(names) <= 34
    caps = client.call("get_capabilities")
    assert caps["tool_count"] == len(names)
    assert caps["tool_profile"] == profile, caps.get("tool_profile")
    assert all(isinstance(tool.get("annotations"), dict) for tool in tools)
    for tool in tools:
        schema = tool["inputSchema"]
        assert schema.get("type") == "object", tool["name"]
        if profile != "compact":
            continue
        assert_typed(schema, schema, tool["name"])
        if tool["name"] in GROUPS and GROUPS[tool["name"]]:
            assert schema.get("additionalProperties") is False, tool["name"]
            assert schema.get("required") == ["operation"], tool["name"]
            tagged = resolve(schema["properties"]["operation"], schema)
            branches = tagged.get("oneOf", tagged.get("anyOf", []))
            actions = []
            for branch in branches:
                branch = resolve(branch, schema)
                tag = resolve(branch["properties"]["action"], schema)
                value = tag.get("const", (tag.get("enum") or [None])[0])
                assert isinstance(value, str) and "action" in branch.get("required", []), tool["name"]
                actions.append(value)
            assert len(actions) == len(set(actions)) and set(actions) == GROUPS[tool["name"]], (tool["name"], actions)
            if tool["name"] != "analyze" and tool["name"] != "arrangement_inspect":
                assert tool["annotations"].get("readOnlyHint") is not True, tool["name"]
    return tools, caps


def help_examples(client, tools, caps, profile):
    schemas = {tool["name"]: tool["inputSchema"] for tool in tools}
    use_jsonschema = importlib.util.find_spec("jsonschema") is not None
    validators = {}
    if use_jsonschema:
        from jsonschema.validators import validator_for
        for name, schema in schemas.items():
            validator = validator_for(schema)
            validator.check_schema(schema)
            validators[name] = validator(schema)
    examples, templates = [], []

    def walk(value, path):
        if isinstance(value, list):
            for index, item in enumerate(value):
                walk(item, f"{path}[{index}]")
        elif isinstance(value, dict):
            # A nested operation's tool:audio_codecs is not an MCP call.
            if isinstance(value.get("tool"), str) and "arguments" in value:
                name, arguments = value["tool"], value["arguments"]
                if name.startswith("<"):
                    templates.append(path)
                else:
                    assert name in schemas, f"Unadvertised mandatory help tool at {path}: {name}"
                    assert isinstance(arguments, dict), path
                    if name in GROUPS and GROUPS[name]:
                        assert set(arguments) == {"operation"}, (path, arguments)
                        assert arguments["operation"].get("action") in GROUPS[name], path
                    if validators:
                        errors = list(validators[name].iter_errors(arguments))
                        assert not errors, {"path": path, "tool": name, "arguments": arguments,
                                            "schema_errors": [error.message for error in errors]}
                    examples.append({"path": path, "tool": name})
            for key, item in value.items():
                walk(item, f"{path}.{key}")

    instructions = client.initialized["result"].get("instructions", "")
    if profile == "compact":
        hidden = LEGACY - COMPACT
        for name in hidden:
            assert not re.search(rf"\b{re.escape(name)}\b", instructions), f"Hidden tool in compact initialization: {name}"
    topics = [None] + list(caps["help_topics"])
    for topic in topics:
        guide = client.call("get_help", {} if topic is None else {"topic": topic})
        walk(guide, f"help[{topic or 'default'}]")
    assert examples, "No concrete structured help examples were found"
    return {"topics": len(topics), "concrete_examples": examples,
            "generic_templates_not_concrete_calls": templates,
            "jsonschema_validation": "passed" if use_jsonschema else "not_run: optional jsonschema is not installed"}


def rejected(client, name, arguments):
    response = client.request("tools/call", {"name": name, "arguments": arguments})
    if "error" in response or response.get("result", {}).get("isError"):
        return
    submitted = response["result"]["structuredContent"]
    assert "job_id" in submitted, f"Invalid request was accepted: {name}: {submitted}"
    job = client.finish(submitted)
    assert job["status"] == "failed", (name, job)


def assert_rejected_unchanged(client, output, name, arguments):
    before, disk = client.call("get_session"), files(output)
    rejected(client, name, arguments)
    assert client.call("get_session") == before, f"Invalid {name} changed session state"
    assert files(output) == disk, f"Invalid {name} changed output files"


def source(path):
    with wave.open(str(path), "wb") as wav:
        wav.setparams((2, 2, 48000, 0, "NONE", "not compressed"))
        data = bytearray()
        for frame in range(144000):
            value = round(2400 * math.sin(2 * math.pi * 220 * frame / 48000)
                          + 700 * math.sin(2 * math.pi * 12000 * frame / 48000))
            data.extend(struct.pack("<hh", value, round(value * 0.8)))
        wav.writeframes(data)


def compact_flow(client, output):
    listing = client.call("list_audio_files", {"library": 0})
    loaded = client.run("load_audio", {"library": 0, "path": listing["files"][0]["path"]})
    source_id = loaded["source_id"]
    client.run("analyze", operation("mastering", source_id=source_id))
    client.run("analyze", operation("studio", audio_id=source_id))
    client.run("reference", operation("load", library=0, path=listing["files"][0]["path"]))
    client.run("reference", operation("clear"))
    master = client.run("master_preview", {"source_id": source_id})
    assert client.call("get_session")["master"]["preview_id"] == master["preview_id"]
    client.run("export_result", operation("mastering", preview_id=master["preview_id"],
                                           name="Compact catalog master", format="wav32f", dither=False))
    assert list(output.glob("*_32f.wav")), "Grouped mastering export produced no Float32 WAV"
    for name, arguments in [
        ("analyze", operation("mastering", source_id=source_id, audio_id=source_id)),
        ("reference", operation("clear", library=0, path="ignored.wav")),
        ("export_result", operation("mastering", preview_id=master["preview_id"], name="must not save", session_id="mixed")),
        ("setup_optional_tool", operation("component", tool="audio_codecs", repair=True, allow_download=False)),
        ("setup_optional_tool", operation("stem_engine", allow_download=False)),
        ("plugin", operation("probe", library=0, path="foreign.vst3", arbitrary=True)),
        ("process_audio", operation("voice_cleanup", audio_id=source_id, selection={"start_seconds": 0})),
        ("studio_history", operation("undo", redo=True)),
    ]:
        assert_rejected_unchanged(client, output, name, arguments)
    client.call("optional_tool_status", {})
    # Legacy calls remain callable without expanding tools/list.
    advertised = client.request("tools/list", {})["result"]["tools"]
    client.run("analyze_audio", {"source_id": source_id})
    assert client.request("tools/list", {})["result"]["tools"] == advertised

    state = client.call("arrangement_edit", operation("create", sample_rate=48000, channels=2))
    state = client.call("arrangement_edit", operation("add_assets", **revision(state),
                         sources=[{"audio_id": source_id, "name": "Explicit recording"}]))
    lane = state["arrangement"]["lanes"][0]
    clip = lane["clips"][0]
    effect = {"id": "catalog-section", "scope": {"kind": "clip", "clip_id": clip["id"]},
              "range": {"start_frame": 48000, "end_frame": 96000}, "bands": state["defaults"]["bands"]}
    effect["bands"][-1]["gain"] = -6
    state = client.call("arrangement_edit", operation("apply", **revision(state), edits=[
        {"kind": "move_clip", "lane_id": lane["id"], "clip_id": clip["id"], "start_frame": 96000},
        {"kind": "set_eq", "effect": effect}]))
    expected = state["arrangement"]
    state = client.call("arrangement_edit", operation("undo", **revision(state)))
    state = client.call("arrangement_edit", operation("redo", **revision(state)))
    assert state["arrangement"] == expected
    client.call("arrangement_inspect", operation("waveform", **revision(state), asset_id=clip["source_id"],
                range={"start_frame": 0, "end_frame": 512}, bins=16))
    client.call("arrangement_inspect", operation("eq_response", **revision(state), effect=effect))
    current = client.call("arrangement_inspect", operation("state"))
    root = client.call("get_session")
    assert {"source", "master", "active_job_id", "studio", "arrangement"} <= set(root)
    assert root["studio"] == client.call("get_studio_session")
    assert root["arrangement"] == current == client.call("get_arrangement")
    for action, fields in [
        ("apply", {**revision(current), "expected_revision": 0, "edits": []}),
        ("undo", {**revision(current), "redo": True}),
        ("apply", {**revision(current), "edits": [], "sources": []}),
    ]:
        assert_rejected_unchanged(client, output, "arrangement_edit", operation(action, **fields))
    saved = client.run("project", operation("save_arrangement", **revision(current)))
    assert current["preview"] is None
    assert_rejected_unchanged(client, output, "project", operation("open_arrangement", project_id=saved["project_id"]))
    assert_rejected_unchanged(client, output, "project", operation("save_studio", **revision(current)))
    return saved, current


def verify(server, evidence):
    assert len(RETAINED) == 19 and len(COMPACT) == 33 and len(LEGACY) == 58
    evidence.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="compact-catalog-", dir=evidence)).resolve()
    binary = root / server.name
    shutil.copy2(server, binary)
    library, output = root / "Music", root / "Exports"
    library.mkdir()
    recording = library / "Catalog recording.wav"
    source(recording)
    original_hash = digest(recording)
    transcript, clients = [], []
    report = {"root": str(root), "server_sha256": digest(binary), "source_sha256_before": original_hash,
              "checks": {}, "physical_playback": False, "installed_connector_changed": False}

    def start(profile="compact"):
        command = [str(binary), "--input-root", str(library), "--output-root", str(output),
                   "--studio-data", str(root / "private-runtime")]
        if profile == "legacy":
            command += ["--tool-profile", "legacy"]
        client = RecordedClient(command, transcript, profile)
        clients.append(client)
        return client

    def close(client):
        client.close()
        clients.remove(client)

    try:
        client = start()
        compact, caps = catalog(client, "compact")
        report["compact_help"] = help_examples(client, compact, caps, "compact")
        report["checks"]["default_33_unique_typed_tools_and_compact_help"] = True
        saved, current = compact_flow(client, output)
        report["checks"]["mastering_flow_and_mixed_request_rejection_preserve_state"] = True
        report["checks"]["legacy_alias_calls_do_not_expand_compact_discovery"] = True
        report["checks"]["atomic_session_views_and_guarded_arrangement_edits"] = True
        report["saved_project"] = saved
        close(client)

        # A fresh connection has no loaded source/master. Reopen needs only owned disk assets.
        retained = root / "Retained original.wav"
        recording.rename(retained)
        client = start()
        before = files(output)
        restored = client.run("project", operation("open_arrangement", project_id=saved["project_id"]))
        assert restored["arrangement"] == current["arrangement"]
        assert restored["history"] == current["history"] and restored["preview"] is None
        assert files(output) == before, "Headless reopen wrote a preview or other output"
        assert client.call("get_session")["source"] is None
        assert {t["name"] for t in client.request("tools/list", {})["result"]["tools"]} == COMPACT
        close(client)
        report["checks"]["cold_compact_reopen_without_originals_preserves_recipe_history_no_render"] = True

        client = start("legacy")
        legacy, caps = catalog(client, "legacy")
        report["legacy_help"] = help_examples(client, legacy, caps, "legacy")
        reopened = client.run("load_arrangement_project", {"project_id": saved["project_id"]})
        assert reopened["arrangement"] == current["arrangement"] and reopened["history"] == current["history"]
        assert client.call("get_arrangement")["preview"] is None
        retained.rename(recording)
        loaded = client.run("load_audio", {"library": 0, "path": recording.name})
        client.run("analyze_audio", {"source_id": loaded["source_id"]})
        assert {t["name"] for t in client.request("tools/list", {})["result"]["tools"]} == LEGACY
        close(client)
        report["checks"]["explicit_legacy_58_original_routes_and_cross_profile_project_reopen"] = True
        compact_bytes = len(json.dumps(compact, ensure_ascii=False, separators=(",", ":")).encode("utf-8"))
        legacy_bytes = len(json.dumps(legacy, ensure_ascii=False, separators=(",", ":")).encode("utf-8"))
        report["catalog_metrics"] = {"measurement": "Canonical compact JSON serialization of tools arrays, UTF-8 bytes; not estimated model tokens or raw transport whitespace",
            "compact_tools": len(compact), "legacy_tools": len(legacy), "compact_bytes": compact_bytes,
            "legacy_bytes": legacy_bytes, "bytes_reduction": legacy_bytes - compact_bytes,
            "compact_to_legacy_ratio": compact_bytes / legacy_bytes}
        report["source_sha256_after"] = digest(recording)
        assert report["source_sha256_after"] == original_hash
        report["passed"] = True
    except Exception as error:
        report.update({"passed": False, "error": repr(error)})
        raise
    finally:
        for client in list(clients):
            try:
                close(client)
            except Exception as error:
                report.setdefault("close_errors", []).append(repr(error))
                report["passed"] = False
        report["transcript"] = transcript
        destination = root / "results.json"
        destination.write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps({"results": str(destination), "passed": report.get("passed"),
                          "checks": report["checks"], "catalog_metrics": report.get("catalog_metrics")}, indent=2))
    assert report["passed"], report.get("close_errors")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    verify(args.server.resolve(), args.evidence.resolve())
