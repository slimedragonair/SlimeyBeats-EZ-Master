"""Validate all staged media before publishing any part of a revision."""

import json
import secrets
import zipfile

from .audio_guard import inspect_wav
from .errors import ProcessingError
from .native_settings import write_private_json
from .project_audio import preview, waveform
from .project_commands import selected_stems
from .project_limits import FORMATS, MAX_ARTIFACTS, MAX_STEMS
from .project_requests import stem_kind
from .project_reports import analysis, report, issues, spectrogram
from .studio_driver import safe_file
from .storage import directory_bytes

MIME = {"wav": "audio/wav", "mp3": "audio/mpeg", "flac": "audio/flac", "zip": "application/zip"}


def artifact(path, fmt, name="master"):
    extension = "mp3" if fmt == "mp3" else "flac" if fmt.startswith("flac") else "zip" if fmt == "zip" else "wav"
    artifact_id = secrets.token_hex(16)
    return {"id": artifact_id, "name": name + "." + extension, "bytes": path.stat().st_size,
            "content_type": MIME[extension], "path": path}


def collect_outputs(command, raw, config, runner):
    operation, project = command.request["operation"], command.project
    entries = raw.get("assets")
    if not isinstance(entries, list) or len(entries) > MAX_STEMS + 2:
        raise ProcessingError("Invalid asset list")
    assets, artifacts = {}, {}
    seen_paths = set()
    clean_analysis = analysis(raw.get("analysis"))
    primary = None
    if isinstance(raw.get("audio"), dict):
        # Read-only commands may describe their immutable committed input.
        primary = raw["audio"].get("path")
    selected_primary = None
    for entry in entries:
        if not isinstance(entry, dict) or entry.get("role") not in ("original", "master", "preview", "stem"):
            raise ProcessingError("Invalid asset role")
        path = safe_file(entry.get("path", ""), command.stage / "out")
        if path in seen_paths:
            raise ProcessingError("Duplicate output")
        seen_paths.add(path)
        fmt = entry.get("format")
        if fmt not in FORMATS:
            raise ProcessingError("Invalid output format")
        bank = operation in ("import_stems", "separate")
        if operation == "separate" and entry["role"] == "original" and project.original_id:
            # Separation preserves its existing source. Importing another bank
            # instead publishes that bank's new raw sum as the source.
            continue
        if operation in ("render", "export", "bundle") and entry["role"] == "master":
            export = artifact(path, fmt)
            artifacts[export["id"]] = export
            continue
        if operation in ("export", "bundle"):
            continue
        if fmt != "wav32f":
            raise ProcessingError("Editable audio must be float WAV")
        info = inspect_wav(path, config)
        rate = entry.get("sample_rate", entry.get("rate"))
        if (type(rate) is not int or rate != info.sample_rate or entry.get("frames") != info.frames
                or entry.get("channels") != info.channels):
            raise ProcessingError("Native geometry mismatch")
        audio_id = secrets.token_hex(16)
        role = "original" if bank and entry["role"] == "original" else "reference" if operation == "reference" else "edited" if command.request.get("as_preview") else "original" if operation in ("import", "clone") else (
            "master" if operation == "master" else "render" if operation == "render" else
            "stem" if entry["role"] == "stem" else "edited")
        name = role
        kind = None
        if role == "stem":
            kind = stem_kind(entry.get("kind"))
            label = entry.get("name", kind)
            if (not isinstance(label, str) or not 1 <= len(label) <= 128 or ".." in label
                    or any(ord(c) < 32 for c in label) or any(c in label for c in "/\\:")):
                raise ProcessingError("Unsafe stem label")
            name = label
        preview_path = path.parent / ("preview-" + audio_id + ".mp3")
        waveform_path = path.parent / ("waveform-" + audio_id + ".json")
        preview(path, preview_path, command, config, runner)
        write_private_json(waveform_path, waveform(path, info, command))
        asset = {"id": audio_id, "role": role, "name": name, "path": path,
                 "preview_path": preview_path, "waveform_path": waveform_path,
                 "frames": info.frames, "sample_rate": info.sample_rate, "channels": info.channels,
                 "duration_seconds": info.frames / info.sample_rate, "bytes": path.stat().st_size,
                 "content_type": "audio/wav", "has_preview": True, "has_waveform": True}
        if kind:
            asset["kind"] = kind
        if clean_analysis is not None and primary and safe_primary(primary) == path:
            asset["analysis"] = clean_analysis
        assets[audio_id] = asset
        if primary and safe_primary(primary) == path:
            selected_primary = audio_id
    if len(project.artifacts) + len(artifacts) > MAX_ARTIFACTS:
        raise ProcessingError("Project media limit")
    if operation in ("import", "clone", "reference", "master", "render", "repair", "voice_cleanup", "spectral_repair", "mix", "region_edit", "replace"):
        if selected_primary is None or len(assets) != 1:
            raise ProcessingError("Missing primary audio")
    if operation in ("export", "bundle") and not artifacts:
        raise ProcessingError("Missing export")
    if operation in ("separate", "import_stems"):
        stems = [asset for asset in assets.values() if asset["role"] == "stem"]
        if not 1 <= len(stems) <= MAX_STEMS or selected_primary is None or assets[selected_primary]["role"] != "edited":
            raise ProcessingError("Missing stems")
        kinds = [asset["kind"] for asset in stems]
        if len(set(kinds)) != len(kinds):
            raise ProcessingError("Duplicate stem kinds")
        originals = [asset["id"] for asset in assets.values() if asset["role"] == "original"]
        needs_original = operation == "import_stems" or project.original_id is None
        if len(originals) > 1 or needs_original and not originals:
            raise ProcessingError("Missing bank original")
    updates = {}
    if operation in ("separate", "import_stems") and originals:
        updates["_bank_original_id"] = originals[0]
    if clean_analysis is not None and operation not in ("export", "bundle"):
        updates["analysis"] = clean_analysis
    if raw.get("report") is not None:
        updates["report"] = report(raw["report"], command.request.get("settings", {}).get("profile", "neutral"))
    if raw.get("issues") is not None:
        updates["issues"] = issues(raw["issues"])
    if raw.get("spectrogram") is not None:
        updates["spectrogram"] = spectrogram(raw["spectrogram"])
    if operation == "suggest":
        from .project_settings import settings_projection
        suggestion = raw.get("suggestion")
        if not isinstance(suggestion, dict):
            raise ProcessingError("Missing EQ suggestion")
        validated = settings_projection({"manual_eq": suggestion.get("bands")})
        updates["_command_result"] = {"bands": validated["manual_eq"]}
    return assets, artifacts, selected_primary, updates


def safe_primary(path):
    from pathlib import Path
    import os
    text = str(path)
    return Path(text[4:] if os.name == "nt" and text.startswith("\\\\?\\") else text)


def bundle(command, config, exports=None):
    target = command.stage / "stems.zip"
    maximum = config.max_job_disk_bytes - directory_bytes(command.directory, config.max_job_disk_bytes, config.max_storage_files)
    with zipfile.ZipFile(target, "x", compression=zipfile.ZIP_STORED) as archive:
        for export in (exports or {}).values():
            _zip_audio(archive, export["path"], export["name"], target, command, maximum)
        metadata = {"manifest.json": command.project.snapshot(), "settings.json": command.request.get("settings", {}),
                    "analysis.json": command.project.analysis, "report.json": command.project.report,
                    "eq.json": {key: value for key, value in command.request.get("settings", {}).items()
                                if key in ("manual_eq", "eq_output_db", "eq_hold_loudness")}}
        for name, value in metadata.items():
            archive.writestr(name, json.dumps(value, allow_nan=False, separators=(",", ":")))
        stems = selected_stems(command.project, command.request) if exports is None or command.request.get("include_stems", False) else []
        for asset, kind in stems:
            path = safe_file(asset["path"], command.project.directory)
            _zip_audio(archive, path, "stems/" + kind + ".wav", target, command, maximum)
    if target.stat().st_size > maximum:
        raise ProcessingError("Bundle limit")
    entry = artifact(target, "zip", "bundle")
    return {}, {entry["id"]: entry}, None, {}


def _zip_audio(archive, path, name, target, command, maximum):
    with path.open("rb") as source, archive.open(name, "w", force_zip64=True) as output:
        while block := source.read(65536):
            if command.cancel.is_set():
                raise ProcessingError("Cancelled")
            output.write(block)
            if target.stat().st_size > maximum:
                raise ProcessingError("Bundle limit")
