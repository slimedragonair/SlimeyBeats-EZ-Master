"""Project native reports onto numeric mastering data; discard all free text."""

import json
import math

from .errors import ProcessingError

METRICS = ("integrated_lufs", "true_peak_dbtp", "sample_peak_dbfs", "rms_dbfs", "crest_db", "lra_lu")
SUMMARY = ("target_lufs", "ceiling_dbtp", "processing_seconds", "punch_loss_db", "safety_attempts")
LIMITER = ("iterations", "reachable", "pre_gain_db", "max_gr_db", "absolute_max_gr_db",
           "trim_db", "out_lufs", "out_tp_dbtp")


def numeric_fields(value, keys):
    if not isinstance(value, dict):
        return {}
    return {key: value[key] for key in keys if key in value
            and type(value[key]) in (int, float, bool) and math.isfinite(value[key])}


def sanitize_report(raw_path, destination, request):
    if raw_path.stat().st_size > 2 * 1024 * 1024:
        raise ProcessingError("Report exceeds its byte limit")
    try:
        raw = json.loads(raw_path.read_text(encoding="utf-8"))
        if not isinstance(raw, dict) or not isinstance(raw.get("input"), dict) or not isinstance(raw.get("output"), dict):
            raise ValueError("Missing measurements")
        clean = {"api_version": 1, "profile": request.profile, "mode": request.mode,
                 "format": request.format, "safety": request.safety, "intensity": request.intensity,
                 "reference_supplied": request.reference_analysis is not None,
                 **numeric_fields(raw, SUMMARY)}
        for key in ("input", "output"):
            clean[key] = {"metrics": numeric_fields(raw[key].get("metrics"), METRICS)}
        clean["limiter"] = numeric_fields(raw.get("limiter"), LIMITER)
        destination.write_text(json.dumps(clean, allow_nan=False, separators=(",", ":")), encoding="utf-8")
    except (ValueError, TypeError, RecursionError, OverflowError) as error:
        raise ProcessingError("Invalid mastering report") from error
