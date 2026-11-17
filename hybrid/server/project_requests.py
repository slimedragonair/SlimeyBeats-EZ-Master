"""Bounded v2 upload, command and Studio option schemas."""

import re

from .errors import ApiError
from .project_limits import FORMATS, INPUT_FORMATS, MAX_STEMS, OPERATIONS, MAX_ARRANGEMENT_ARCHIVE_BYTES
from .project_settings import boolean, choice, number, object_fields, settings_projection

ID = re.compile(r"[a-f0-9]{32}\Z")
STEM_KIND = re.compile(r"(?:vocals|instrumental|drums|bass|other|guitar|piano|synth|strings|percussion|part_[1-9][0-9]{0,9})\Z")


def identifier(value):
    if not isinstance(value, str) or ID.fullmatch(value) is None:
        raise ApiError(400, "invalid_id")
    return value


def stem_kind(value):
    if (not isinstance(value, str) or STEM_KIND.fullmatch(value) is None
            or value.startswith("part_") and int(value[5:]) > 4294967295):
        raise ApiError(400, "invalid_stem_kind")
    return value


def upload_request(value, config):
    object_fields(value, ("filename", "bytes", "role", "label", "kind"), ("filename", "bytes", "role"))
    filename = value["filename"]
    if (not isinstance(filename, str) or not 1 <= len(filename) <= 255 or ".." in filename
            or any(ord(c) < 32 or ord(c) == 127 for c in filename) or any(c in filename for c in "/\\:")
            or filename.endswith((" ", "."))):
        raise ApiError(400, "invalid_filename")
    role = choice(value["role"], ("original", "reference", "stem", "stem_bundle", "arrangement_project", "arrangement_archive"))
    if "kind" in value:
        if role != "stem":
            raise ApiError(400, "invalid_stem_kind")
        stem_kind(value["kind"])
    label = value.get("label")
    if label is not None and (role != "stem" or not isinstance(label, str) or not 1 <= len(label) <= 128
                              or any(ord(c) < 32 or ord(c) == 127 for c in label) or any(c in label for c in "/\\:") or ".." in label):
        raise ApiError(400, "invalid_stem_label")
    extension = filename.rsplit(".", 1)[-1].lower()
    if "." not in filename or extension not in (("zip",) if role in ("stem_bundle", "arrangement_project", "arrangement_archive") else INPUT_FORMATS):
        raise ApiError(400, "unsupported_input")
    size = value["bytes"]
    if type(size) is not int or size <= 0:
        raise ApiError(400, "invalid_bytes")
    if size > config.v2_max_upload_bytes:
        raise ApiError(413, "upload_limit")
    if role == 'arrangement_archive' and size > MAX_ARRANGEMENT_ARCHIVE_BYTES:
        raise ApiError(413, 'arrangement_archive_size_limit')
    return extension, size, role


def region(value, maximum):
    object_fields(value, ("start_seconds", "end_seconds"), ("start_seconds", "end_seconds"))
    clean = {name: number(item, 0, maximum) for name, item in value.items()}
    if clean["start_seconds"] >= clean["end_seconds"]:
        raise ApiError(400, "invalid_region")
    return clean


def studio_options(value, kind, maximum):
    schemas = {
        "voice": ({"highpass_hz": (0, 300), "noise_reduction": (0, 1), "deess": (0, 1),
                   "gate_threshold_db": (-80, -15), "output_gain_db": (-12, 12), "ceiling_dbtp": (-12, 0)},
                  {"hold_loudness", "protect_peaks"}, {"noise_region"}, ()),
        "repair": ({"start_seconds": (0, maximum), "end_seconds": (0, maximum), "strength": (0, 1)},
                   set(), {"action"}, ("action", "start_seconds", "end_seconds", "strength")),
        "spectral": ({"start_seconds": (0, maximum), "end_seconds": (0, maximum), "low_hz": (0, 192000),
                      "high_hz": (0, 192000), "reduction_db": (-60, 0)},
                     set(), {"mode"}, ("start_seconds", "end_seconds", "low_hz", "high_hz", "reduction_db", "mode")),
        "mix": ({"output_gain_db": (-24, 12), "ceiling_dbtp": (-12, 0)}, {"protect_peaks"}, {"tracks"}, ()),
    }
    numbers, bools, special, required = schemas[kind]
    object_fields(value, {*numbers, *bools, *special}, required)
    clean = {}
    for key, item in value.items():
        if key in numbers:
            clean[key] = None if kind == "voice" and key == "gate_threshold_db" and item is None else number(item, *numbers[key])
        elif key in bools:
            clean[key] = boolean(item)
        elif key == "noise_region":
            clean[key] = None if item is None else region(item, maximum)
        elif key == "action":
            clean[key] = choice(item, ("declip", "reduce_low_mids"))
        elif key == "mode":
            clean[key] = choice(item, ("attenuate", "heal"))
        elif key == "tracks":
            if not isinstance(item, list) or not 1 <= len(item) <= MAX_STEMS:
                raise ApiError(400, "invalid_mix")
            clean[key] = []
            for track in item:
                object_fields(track, ("kind", "gain_db", "balance", "mute", "solo"),
                              ("kind", "gain_db", "balance", "mute", "solo"))
                clean[key].append({"kind": stem_kind(track["kind"]), "gain_db": number(track["gain_db"], -60, 12),
                                   "balance": number(track["balance"], -1, 1), "mute": boolean(track["mute"]),
                                   "solo": boolean(track["solo"])})
            if len({track["kind"] for track in clean[key]}) != len(item):
                raise ApiError(400, "duplicate_stem_kind")
    if "start_seconds" in clean and clean["start_seconds"] >= clean["end_seconds"]:
        raise ApiError(400, "invalid_region")
    if kind == "spectral" and clean["low_hz"] >= clean["high_hz"]:
        raise ApiError(400, "invalid_region")
    if kind == "voice" and 0 < clean.get("highpass_hz", 0) < 20:
        raise ApiError(400, "invalid_number")
    return clean


def command_request(value, config):
    object_fields(value, ("operation", "base_revision", "upload_ids", "audio_id", "settings", "voice", "repair",
                          "spectral", "stems", "mix", "separation_mode", "format", "redo", "region", "spectrogram", "source_project_id",
                          "kind", "include_stems", "as_preview", "reference_id"),
                  ("operation", "base_revision"))
    operation = choice(value["operation"], (*OPERATIONS, "history", "replace", "promote"))
    revision = value["base_revision"]
    if type(revision) is not int or not 0 <= revision <= 2**53 - 1:
        raise ApiError(400, "invalid_revision")
    clean = {"operation": operation, "base_revision": revision}
    for key, item in value.items():
        if key in clean:
            continue
        if key in ("audio_id", "source_project_id"):
            clean[key] = identifier(item)
        elif key == "reference_id":
            clean[key] = None if item is None else identifier(item)
        elif key == "upload_ids":
            if not isinstance(item, list) or not 1 <= len(item) <= MAX_STEMS or len(set(map(str, item))) != len(item):
                raise ApiError(400, "invalid_uploads")
            clean[key] = [identifier(entry) for entry in item]
        elif key == "settings":
            clean[key] = settings_projection(item)
        elif key in ("voice", "repair", "spectral", "mix"):
            clean[key] = studio_options(item, key, config.v2_max_audio_seconds)
        elif key == "stems":
            if not isinstance(item, list) or not 1 <= len(item) <= MAX_STEMS:
                raise ApiError(400, "invalid_stems")
            clean[key] = []
            for entry in item:
                object_fields(entry, ("audio_id", "kind"), ("audio_id", "kind"))
                clean[key].append({"audio_id": identifier(entry["audio_id"]), "kind": stem_kind(entry["kind"])})
            if len({stem["kind"] for stem in clean[key]}) != len(item):
                raise ApiError(400, "duplicate_stem_kind")
        elif key == "format":
            clean[key] = choice(item, FORMATS)
        elif key == "separation_mode":
            clean[key] = choice(item, ("vocal_instrumental", "four_stems"))
        elif key == "redo":
            clean[key] = boolean(item)
        elif key in ("include_stems", "as_preview"):
            clean[key] = boolean(item)
        elif key == "kind":
            clean[key] = choice(item, ("mastering", "studio"))
        elif key == "spectrogram":
            object_fields(item, ("columns", "rows"))
            clean[key] = {}
            for name, low, high in (("columns", 16, 1024), ("rows", 16, 256)):
                if name in item:
                    if type(item[name]) is not int:
                        raise ApiError(400, "invalid_spectrogram")
                    clean[key][name] = number(item[name], low, high)
        elif key == "region":
            object_fields(item, ("start_seconds", "end_seconds", "operation"),
                          ("start_seconds", "end_seconds", "operation"))
            clean[key] = region({name: item[name] for name in ("start_seconds", "end_seconds")}, config.v2_max_audio_seconds)
            action = item["operation"]
            object_fields(action, ("kind", "db"), ("kind",))
            kind = choice(action["kind"], ("silence", "fade_in", "fade_out", "reverse", "gain"))
            clean[key]["operation"] = {"kind": kind}
            if kind == "gain":
                clean[key]["operation"]["db"] = number(action.get("db"), -60, 12)
            elif "db" in action:
                raise ApiError(400, "invalid_region")
    if operation in ("repair", "voice_cleanup", "spectral_repair"):
        required = {"repair": "repair", "voice_cleanup": "voice", "spectral_repair": "spectral"}[operation]
        if required not in clean:
            raise ApiError(400, "missing_tool_options")
    if operation == "region_edit" and "region" not in clean:
        raise ApiError(400, "missing_tool_options")
    if operation == "clone" and not {"source_project_id", "audio_id"} <= set(clean):
        raise ApiError(400, "clone_source_required")
    if operation == "spectrogram":
        display = clean.setdefault("spectrogram", {})
        display.setdefault("columns", 320)
        display.setdefault("rows", 128)
        if display["columns"] * display["rows"] > 65536:
            raise ApiError(413, "spectrogram_limit")
    applicable = {"voice": ("voice_cleanup",), "repair": ("repair",), "spectral": ("spectral_repair",),
                  "mix": ("mix",), "region": ("region_edit",), "spectrogram": ("spectrogram",),
                  "redo": ("history",), "source_project_id": ("clone",), "separation_mode": ("separate",),
                  "upload_ids": ("import", "reference", "import_stems"), "stems": ("mix", "bundle"),
                  "kind": ("bundle",), "include_stems": ("bundle",), "as_preview": ("import",),
                  "reference_id": ("master",)}
    if any(key in clean and operation not in operations for key, operations in applicable.items()):
        raise ApiError(400, "inapplicable_option")
    return clean
