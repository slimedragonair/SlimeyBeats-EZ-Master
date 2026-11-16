"""Resolve explicit project-owned reference choices without accepting public Analysis."""

import json
import math

from .errors import ApiError
from .reference_analysis import DB_FIELDS, MAX_REFERENCE_BYTES, fields, integer, number, vector


def selected_reference_id(project, request):
    # get's default applies only to omission: explicit null means no matching.
    return request.get("reference_id", project.reference_id)


def reference_projection(project, request, config):
    reference_id = selected_reference_id(project, request)
    if reference_id is None:
        return None
    asset = project.assets.get(reference_id)
    if asset is None:
        raise ApiError(404, "reference_not_found")
    value = asset.get("analysis")
    try:
        if value is None or len(json.dumps(value, allow_nan=False, separators=(",", ":")).encode()) > MAX_REFERENCE_BYTES:
            raise ValueError("Missing or oversized private Analysis")
        fields(value, ("duration_s", "sample_rate", "channels", "metrics", "spectrum", "measured_db", "deviation_db", "valid"),
               ("issues", "suggested_profile"))
        duration = number(value["duration_s"], 0, config.v2_max_audio_seconds)
        rate, channels = integer(value["sample_rate"], 8000, 384000), integer(value["channels"], 1, 2)
        samples = duration * rate * channels
        if duration <= 0 or samples * 4 > config.v2_max_decoded_bytes + 0.001:
            raise ValueError("Invalid reference geometry")
        metrics = value["metrics"]
        fields(metrics, (*DB_FIELDS, "correlation", "dc_offset", "clipped_samples"))
        clean_metrics = {key: number(metrics[key], -1000, 1000) for key in DB_FIELDS}
        clean_metrics.update(correlation=number(metrics["correlation"], -1.000001, 1.000001),
                             dc_offset=vector(metrics["dc_offset"], channels, -1e18, 1e18),
                             clipped_samples=integer(metrics["clipped_samples"], 0, math.ceil(samples)))
        spectrum = value["spectrum"]
        fields(spectrum, ("freqs", "db", "cutoff_hz", "quiet_hf_flatness", "noise_candidate", "sibilance_spike_fraction", "octave_power"))
        # Native Analysis keeps its fixed 240-bin display axis even above the
        # Nyquist of low-rate audio; cutoff and valid bands carry that geometry.
        # This private/native contract differs from v1 browser-supplied Analysis.
        freqs = vector(spectrum["freqs"], 240, 0, 192000)
        if freqs[0] <= 0 or any(a >= b for a, b in zip(freqs, freqs[1:])) or type(spectrum["noise_candidate"]) is not bool:
            raise ValueError("Invalid reference spectrum")
        clean_spectrum = {"freqs": freqs, "db": vector(spectrum["db"], 240, -1000, 1000),
                          "cutoff_hz": None if spectrum["cutoff_hz"] is None else number(spectrum["cutoff_hz"], 0, rate / 2),
                          "quiet_hf_flatness": number(spectrum["quiet_hf_flatness"], 0, 1.000001),
                          "noise_candidate": spectrum["noise_candidate"],
                          "sibilance_spike_fraction": number(spectrum["sibilance_spike_fraction"], 0, 1),
                          "octave_power": vector(spectrum["octave_power"], 10, 0, 1e18)}
        valid = value["valid"]
        if not isinstance(valid, list) or len(valid) != 10 or any(type(item) is not bool for item in valid):
            raise ValueError("Invalid reference bands")
        return {"duration_s": duration, "sample_rate": rate, "channels": channels, "metrics": clean_metrics,
                "spectrum": clean_spectrum, "measured_db": vector(value["measured_db"], 10, -1000, 1000),
                "deviation_db": vector(value["deviation_db"], 10, -1000, 1000), "valid": list(valid),
                "issues": [], "suggested_profile": "neutral"}
    except (ValueError, TypeError, OverflowError, RecursionError) as error:
        raise ApiError(409, "reference_analysis_required") from error
