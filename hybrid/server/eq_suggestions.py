"""Project native proposals into bounded bands, evidence and fixed safe notes."""

from .errors import ApiError, ProcessingError
from .high_frequency import summary
from .project_settings import settings_projection


def suggestion(value, analysis):
    if not isinstance(value, dict) or not isinstance(analysis, dict):
        raise ProcessingError("Missing EQ suggestion")
    try:
        bands = settings_projection({"manual_eq": value.get("bands")})["manual_eq"]
        evidence = summary(value.get("high_frequency"), analysis.get("sample_rate"), analysis.get("duration_s"))
    except (ValueError, TypeError, OverflowError, ApiError) as error:
        raise ProcessingError("Invalid EQ suggestion") from error
    notes = ["Conservative, deterministic starting point with 25% tonal-profile influence. Compare by listening before applying; a profile is not a quality score."]
    if evidence is None:
        notes.append("High-frequency evidence is unavailable; no excess-high-frequency claim is made.")
    elif evidence["excess_candidate"]:
        notes.append(f"Repeated or sustained broadband energy is concentrated at {evidence['band_low_hz'] / 1000:g}–{evidence['band_high_hz'] / 1000:g} kHz in {evidence['candidate_fraction'] * 100:.0f}% of sampled active windows. This is a review candidate, not proof of audible harshness, damage or instrument identity.")
        if bands[6]["on"]:
            notes.append("The shared proposal limits combined upper-treble attenuation to 3 dB. A shelf affects all audio in that region and cannot restore missing instrument detail.")
    else:
        notes.append("No repeated excess-high-frequency candidate met the detector's evidence requirements.")
    if evidence is not None and evidence["sampling_capped"]:
        notes.append("Analysis sampled bounded windows; candidate seconds describe observed windows, not continuous coverage.")
    return {"bands": bands, "notes": notes, "high_frequency": evidence}
