"""Strict public job schema: no paths, custom profiles, URLs, or CLI arguments."""

from dataclasses import dataclass
import math
import re

from .errors import ApiError
from .profile_blends import builtin_blend
from .reference_analysis import project_reference


@dataclass(frozen=True)
class MasterRequest:
    total_bytes: int
    extension: str
    profile: str
    format: str
    mode: str
    target_lufs: float | None
    ceiling_dbtp: float
    intensity: float
    safety: bool
    reference_analysis: dict | None


def parse_master_request(value, config):
    keys = {"filename", "bytes", "profile", "format", "mode", "target_lufs",
            "ceiling_dbtp", "intensity", "safety", "reference_analysis", "consent"}
    if not isinstance(value, dict) or set(value) - keys or not {"filename", "bytes", "consent"} <= set(value):
        raise ApiError(400, "Invalid job fields")
    if value["consent"] is not True:
        raise ApiError(400, "Explicit temporary processing consent is required")
    filename = value["filename"]
    if (not isinstance(filename, str) or not 1 <= len(filename) <= 255
            or any(ord(c) < 32 for c in filename) or any(c in filename for c in "/\\:")
            or ".." in filename or filename.endswith((" ", "."))):
        raise ApiError(400, "Invalid audio filename")
    extension = filename.rsplit(".", 1)[-1].lower()
    if extension not in ("wav", "mp3") or not re.search(r"\.(wav|mp3)$", filename, re.I):
        raise ApiError(400, "Only PCM/float WAV and MP3 inputs are supported")
    total = value["bytes"]
    if type(total) is not int or total <= 0:
        raise ApiError(400, "bytes must be a positive integer")
    if total > config.max_upload_bytes:
        raise ApiError(413, "Upload exceeds the byte limit")
    profile = builtin_blend(value.get("profile", "neutral"))
    fmt = value.get("format", "wav24")
    mode = value.get("mode", "streaming_safe")
    if fmt not in ("wav24", "mp3", "wav32f") or mode not in ("streaming_safe", "loud"):
        raise ApiError(400, "Invalid format or mastering mode")
    safety = value.get("safety", True)
    if type(safety) is not bool:
        raise ApiError(400, "safety must be a boolean")
    numbers = {}
    for key, default, low, high in (("target_lufs", None, -40, -5),
                                     ("ceiling_dbtp", -1, -12, 0), ("intensity", 1, 0, 1.5)):
        number = value.get(key, default)
        if number is None and key == "target_lufs":
            numbers[key] = None
            continue
        if type(number) not in (int, float) or not low <= number <= high or not math.isfinite(number):
            raise ApiError(400, "Invalid mastering numeric setting")
        numbers[key] = float(number)
    reference = project_reference(value.get("reference_analysis"), config)
    return MasterRequest(total, extension, profile, fmt, mode, **numbers,
                         safety=safety, reference_analysis=reference)
