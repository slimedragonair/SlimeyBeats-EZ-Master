"""Validate and copy browser Analysis data; never retain reference free text."""

import json
import math

from .errors import ApiError

MAX_REFERENCE_BYTES = 16384
DB_FIELDS = ("integrated_lufs", "true_peak_dbtp", "sample_peak_dbfs", "lra_lu",
             "rms_dbfs", "crest_db", "plr_db")


def number(value, low, high):
    if type(value) not in (int, float) or not low <= value <= high or not math.isfinite(value):
        raise ValueError("Invalid reference number")
    return float(value)


def integer(value, low, high):
    if type(value) is not int or not low <= value <= high:
        raise ValueError("Invalid reference integer")
    return value


def vector(value, size, low, high):
    if not isinstance(value, list) or len(value) != size:
        raise ValueError("Invalid reference vector")
    return [number(item, low, high) for item in value]


def fields(value, required, optional=()):
    if not isinstance(value, dict) or not set(required) <= value.keys() or value.keys() - set(required) - set(optional):
        raise ValueError("Invalid reference fields")


def project_reference(value, config):
    if value is None:
        return None
    try:
        if len(json.dumps(value, allow_nan=False, separators=(",", ":")).encode("utf-8")) > MAX_REFERENCE_BYTES:
            raise ValueError("Reference is too large")
        fields(value, ("duration_s", "sample_rate", "channels", "metrics", "spectrum",
                       "measured_db", "deviation_db", "valid"), ("issues", "suggested_profile"))
        duration = number(value["duration_s"], 0, min(1800, config.max_audio_seconds))
        rate = integer(value["sample_rate"], 8000, 384000)
        channels = integer(value["channels"], 1, 2)
        samples = duration * rate * channels
        if duration <= 0 or samples * 4 > config.max_decoded_bytes:
            raise ValueError("Invalid reference geometry")
        metrics = value["metrics"]
        fields(metrics, (*DB_FIELDS, "correlation", "dc_offset", "clipped_samples"))
        clean_metrics = {key: number(metrics[key], -300, 300) for key in DB_FIELDS}
        clean_metrics.update(correlation=number(metrics["correlation"], -1, 1),
                             dc_offset=vector(metrics["dc_offset"], channels, -1e6, 1e6),
                             clipped_samples=integer(metrics["clipped_samples"], 0, math.ceil(samples)))
        spectrum = value["spectrum"]
        fields(spectrum, ("freqs", "db", "cutoff_hz", "quiet_hf_flatness", "noise_candidate",
                          "sibilance_spike_fraction", "octave_power"))
        freqs = vector(spectrum["freqs"], 240, 0, rate / 2)
        if freqs[0] <= 0 or any(a >= b for a, b in zip(freqs, freqs[1:])):
            raise ValueError("Invalid reference frequencies")
        if type(spectrum["noise_candidate"]) is not bool:
            raise ValueError("Invalid reference flag")
        clean_spectrum = {
            "freqs": freqs, "db": vector(spectrum["db"], 240, -300, 300),
            "cutoff_hz": None if spectrum["cutoff_hz"] is None else number(spectrum["cutoff_hz"], 0, rate / 2),
            "quiet_hf_flatness": number(spectrum["quiet_hf_flatness"], 0, 1.000001),
            "noise_candidate": spectrum["noise_candidate"],
            "sibilance_spike_fraction": number(spectrum["sibilance_spike_fraction"], 0, 1),
            "octave_power": vector(spectrum["octave_power"], 10, 0, 1e12),
        }
        valid = value["valid"]
        if not isinstance(valid, list) or len(valid) != 10 or any(type(item) is not bool for item in valid):
            raise ValueError("Invalid reference bands")
        return {"duration_s": duration, "sample_rate": rate, "channels": channels,
                "metrics": clean_metrics, "spectrum": clean_spectrum,
                "measured_db": vector(value["measured_db"], 10, -300, 300),
                "deviation_db": vector(value["deviation_db"], 10, -300, 300),
                "valid": list(valid), "issues": [], "suggested_profile": "neutral"}
    except (ValueError, TypeError, OverflowError, RecursionError) as error:
        raise ApiError(400, "Invalid reference analysis") from error
