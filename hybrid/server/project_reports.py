"""Bounded numeric native projections and fixed issue enums, never raw text."""

import math
import re

from .errors import ProcessingError
from .config import PROFILES

KEY = re.compile(r"[a-z][a-z0-9_]{0,63}\Z")
PRIVATE = {"path", "file", "filename", "name", "logs", "log", "stderr", "stdout", "configuration", "config",
           "out_dir", "project_root", "exports", "notes", "message", "title", "description", "tool_metadata"}
ISSUE_TEXT = {
    "clipping": ("Full-scale sample candidate", "Inspect this region for clipping before choosing repair."),
    "silence": ("Quiet region", "Review this quiet region against the intended arrangement."),
    "possible_ending": ("Possible ending", "Review this region against the intended ending."),
    "low_mid_buildup": ("Low-mid energy concentration", "Review the sustained concentration before choosing repair."),
}
ANALYSIS_ISSUE_TEXT = {
    "mud": "Low-mid energy differs from the selected tonal target.",
    "harshness": "Upper-mid energy differs from the selected tonal target.",
    "sibilance": "High-frequency bursts may need review.",
    "fizz": "Elevated high-frequency energy may need review.",
    "clipping": "Samples reach digital full scale; inspect for clipping.",
    "noise": "Quiet high-frequency content may contain stationary noise.",
    "stereo": "Stereo correlation indicates possible phase cancellation.",
}


def numeric(value, depth=0, budget=None):
    budget = [20000] if budget is None else budget
    budget[0] -= 1
    if budget[0] < 0 or depth > 12:
        raise ProcessingError("Report bounds")
    if value is None or type(value) is bool:
        return value
    if type(value) in (int, float):
        if not math.isfinite(value) or abs(value) > 1e18:
            raise ProcessingError("Report number")
        return value
    if isinstance(value, list):
        return [numeric(item, depth + 1, budget) for item in value if not isinstance(item, str)]
    if isinstance(value, dict):
        return {key: numeric(item, depth + 1, budget) for key, item in value.items()
                if KEY.fullmatch(key) and key not in PRIVATE and not isinstance(item, str)}
    return None


def analysis(value):
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ProcessingError("Invalid analysis")
    clean = numeric({key: item for key, item in value.items() if key not in ("issues", "suggested_profile")})
    raw_issues = value.get("issues", [])
    if not isinstance(raw_issues, list) or len(raw_issues) > 64:
        raise ProcessingError("Analysis issue bounds")
    severities = {}
    for issue in raw_issues:
        if not isinstance(issue, dict):
            continue
        key, severity = issue.get("key"), issue.get("severity")
        if (not isinstance(key, str) or key not in ANALYSIS_ISSUE_TEXT
                or type(severity) not in (int, float) or not math.isfinite(severity) or not 0 <= severity <= 100):
            continue
        severities[key] = max(severities.get(key, 0), severity)
    clean["issues"] = [{"key": key, "severity": severities[key], "detail": detail}
                       for key, detail in ANALYSIS_ISSUE_TEXT.items() if key in severities]
    suggested = value.get("suggested_profile")
    clean["suggested_profile"] = suggested if isinstance(suggested, str) and suggested in PROFILES else "neutral"
    return clean


def report(value, profile="neutral"):
    if (not isinstance(value, dict) or not isinstance(value.get("input"), dict)
            or not isinstance(value.get("output"), dict)):
        raise ProcessingError("Invalid mastering report")
    clean = numeric(value)
    # Live EQ deserializes these as the shared Analysis type. Generic numeric
    # redaction leaves incomplete Issue objects and removes suggested_profile.
    # Restore the safe typed projection without exposing native free text.
    clean.update(api_version=2, input=analysis(value["input"]), output=analysis(value["output"]),
                 profile=PROFILES.get(profile, profile), notes=[], log=[], exports=[])
    return clean


def issues(value):
    if value is None:
        return None
    if not isinstance(value, dict) or not isinstance(value.get("issues"), list) or len(value["issues"]) > 256:
        raise ProcessingError("Invalid issue report")
    clean = numeric({key: item for key, item in value.items() if key != "issues"})
    items = clean["issues"] = []
    for index, issue in enumerate(value["issues"]):
        if not isinstance(issue, dict):
            raise ProcessingError("Invalid issue")
        kind = issue.get("kind")
        if kind not in ("clipping", "silence", "possible_ending", "low_mid_buildup"):
            raise ProcessingError("Invalid issue kind")
        entry = numeric(issue)
        entry.update(id=str(index), kind=kind)
        entry["title"], entry["description"] = ISSUE_TEXT[kind]
        severity = issue.get("severity")
        entry["severity"] = severity if severity in ("info", "warning", "error", "low", "medium", "high") else "info"
        repair = issue.get("repair")
        entry["repair"] = repair if repair in ("declip", "reduce_low_mids") else None
        items.append(entry)
    return clean


def spectrogram(value):
    if not isinstance(value, dict):
        raise ProcessingError("Invalid spectrogram")
    columns, rows = value.get("columns"), value.get("rows")
    if (type(columns) is not int or type(rows) is not int or not 16 <= columns <= 1024
            or not 16 <= rows <= 256 or len(value.get("frequencies_hz", [])) != rows
            or len(value.get("levels_dbfs", [])) != rows * columns):
        raise ProcessingError("Spectrogram bounds")
    clean = {"columns": columns, "rows": rows}
    for key, low, high in (("duration_seconds", 0, 14400), ("frequencies_hz", 0, 192000), ("levels_dbfs", -300, 300)):
        item = value.get(key)
        values = item if isinstance(item, list) else [item]
        if any(type(n) not in (int, float) or not math.isfinite(n) or not low <= n <= high for n in values):
            raise ProcessingError("Spectrogram number")
        clean[key] = item
    return clean
