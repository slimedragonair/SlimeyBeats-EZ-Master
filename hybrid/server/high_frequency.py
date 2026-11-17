"""Validate the optional shared HF evidence without retaining arbitrary text."""

import math


FIELDS = frozenset(("version", "band_low_hz", "band_high_hz", "reference_low_hz", "reference_high_hz",
                    "sampled_windows", "active_windows", "candidate_windows", "candidate_groups",
                    "candidate_fraction", "observed_candidate_seconds", "mean_high_power_fraction",
                    "p90_high_to_presence_db", "mean_high_flatness", "excess_candidate", "suggested_cut_db",
                    "sampling_capped"))


def number(value, low, high):
    if type(value) not in (int, float) or not math.isfinite(value) or not low <= value <= high:
        raise ValueError("Invalid high-frequency number")
    return value


def summary(value, sample_rate, duration_s):
    if value is None:
        return None
    if not isinstance(value, dict) or set(value) != FIELDS:
        raise ValueError("Invalid high-frequency fields")
    if type(sample_rate) is not int or not 8000 <= sample_rate <= 384000:
        raise ValueError("Invalid high-frequency rate")
    number(duration_s, 2, float("inf"))
    if type(value["version"]) is not int or value["version"] != 1:
        raise ValueError("Invalid high-frequency version or duration")
    clean = {"version": 1}
    for key in ("band_low_hz", "band_high_hz", "reference_low_hz", "reference_high_hz"):
        clean[key] = number(value[key], 0, sample_rate / 2)
    # Mirror HighFrequencySummary::valid; browser metadata must satisfy the
    # same measurement contract before it can reach native reference matching.
    if not (clean["band_low_hz"] == 10000 and 14000 <= clean["band_high_hz"] <= 16000
            and clean["band_high_hz"] <= sample_rate * 0.49 + 0.001
            and clean["reference_low_hz"] == 2000 and clean["reference_high_hz"] == 8000):
        raise ValueError("Invalid high-frequency bands")
    for key in ("sampled_windows", "active_windows", "candidate_windows", "candidate_groups"):
        item = value[key]
        if type(item) is not int or not 0 <= item <= 3072:
            raise ValueError("Invalid high-frequency count")
        clean[key] = item
    if not (clean["candidate_windows"] <= clean["active_windows"] <= clean["sampled_windows"]
            and clean["candidate_groups"] <= clean["candidate_windows"] // 2):
        raise ValueError("Invalid high-frequency window counts")
    for key in ("candidate_fraction", "mean_high_power_fraction", "mean_high_flatness"):
        clean[key] = number(value[key], 0, 1)
    fraction = clean["candidate_windows"] / max(clean["active_windows"], 1)
    if abs(clean["candidate_fraction"] - fraction) >= 1e-6:
        raise ValueError("Invalid high-frequency candidate fraction")
    clean["observed_candidate_seconds"] = number(value["observed_candidate_seconds"], 0, duration_s + 0.001)
    clean["p90_high_to_presence_db"] = number(value["p90_high_to_presence_db"], -120, 120)
    clean["suggested_cut_db"] = number(value["suggested_cut_db"], -2.5, 0)
    for key in ("excess_candidate", "sampling_capped"):
        if type(value[key]) is not bool:
            raise ValueError("Invalid high-frequency flag")
        clean[key] = value[key]
    if clean["excess_candidate"]:
        if not (clean["candidate_windows"] >= 8 and clean["observed_candidate_seconds"] >= 0.5
                and clean["candidate_groups"] > 0 and clean["suggested_cut_db"] <= -0.75):
            raise ValueError("Invalid high-frequency candidate support")
    elif clean["suggested_cut_db"] != 0:
        raise ValueError("Unsupported high-frequency cut")
    return clean
