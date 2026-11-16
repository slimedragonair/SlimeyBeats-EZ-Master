"""Safe native settings projection. No public field can select a local path."""

import math

from .errors import ApiError
from .profile_blends import builtin_blend

MODULE_PARAMETERS = {
    "headroom": {"gain_db": (-24, 0)}, "declip": {"max_run": (1, 256)},
    "cleanup": {"hpf_hz": (5, 300)}, "denoise": {"amount": (0, 1)},
    "hf_rebuild": {"mix_db": (-60, 0)},
    "static_eq": {"fraction": (0, 1), "max_db": (0, 12), "tolerance_db": (0, 12),
                  "max_boost_db": (0, 12), "response_max_db": (0, 12)},
    "resonance": {"max_notches": (0, 16), "max_db": (0, 12)},
    "low_end": {"psycho_bass_db": (-60, 0)},
    "low_punch": {"attack_db": (0, 12), "sustain_db": (0, 12), "sub_duck_db": (0, 12)},
    "dynamic_eq": {"mud_max_db": (0, 12), "harsh_max_db": (0, 12), "total_cut_max_db": (0, 24)},
    "deesser": {"lo_hz": (1000, 20000), "hi_hz": (1000, 20000), "max_db": (0, 12)},
    "glue": {"ratio": (1, 10), "attack_ms": (0.1, 200), "release_ms": (5, 2000)},
    "saturation": {"drive_db": (0, 12), "mix": (0, 1)}, "stereo": {"width": (0, 2)},
    "multiband": {}, "clipper": {"drive_db": (0, 12)},
    "reference_match": {"max_db": (0, 12)},
    "limiter": {"lookahead_ms": (0.1, 20), "release_ms": (1, 1000),
                "release_slow_ms": (1, 5000), "max_makeup_db": (0, 40), "max_gr_abort_db": (0, 12)},
}
NUMBERS = {"target_lufs": (-40, -5), "ceiling_dbtp": (-12, 0), "intensity": (0, 1.5),
           "match_strength": (0, 1.5), "max_makeup_db": (0, 40),
           "max_limiter_gr_db": (0, 12), "eq_output_db": (-12, 12)}
BOOLEANS = {"match_loudness", "match_width", "dither", "safety", "eq_hold_loudness"}


def object_fields(value, keys, required=()):
    if not isinstance(value, dict) or set(value) - set(keys) or not set(required) <= set(value):
        raise ApiError(400, "invalid_fields")
    return value


def number(value, low, high):
    if type(value) not in (int, float) or not math.isfinite(value) or not low <= value <= high:
        raise ApiError(400, "invalid_number")
    return value


def boolean(value):
    if type(value) is not bool:
        raise ApiError(400, "invalid_boolean")
    return value


def choice(value, choices):
    if not isinstance(value, str) or value not in choices:
        raise ApiError(400, "invalid_option")
    return value


def settings_projection(value):
    keys = {*NUMBERS, *BOOLEANS, "profile", "mode", "output_sr", "disabled", "manual_eq", "modules"}
    object_fields(value, keys)
    clean = {}
    for key, item in value.items():
        if key in NUMBERS:
            clean[key] = None if key == "target_lufs" and item is None else number(item, *NUMBERS[key])
        elif key in BOOLEANS:
            clean[key] = boolean(item)
        elif key == "profile":
            clean[key] = builtin_blend(item)
        elif key == "mode":
            clean[key] = choice(item, ("streaming_safe", "loud"))
        elif key == "output_sr":
            if type(item) is not int or item != 0 and not 8000 <= item <= 192000:
                raise ApiError(400, "invalid_sample_rate")
            clean[key] = item
        elif key == "disabled":
            if not isinstance(item, list) or len(item) > len(MODULE_PARAMETERS):
                raise ApiError(400, "invalid_modules")
            clean[key] = [choice(module, MODULE_PARAMETERS) for module in item]
        elif key == "manual_eq":
            if not isinstance(item, list) or len(item) != 7:
                raise ApiError(400, "invalid_equalizer")
            clean[key] = []
            for band in item:
                object_fields(band, ("type", "freq", "gain", "q", "on"), ("type", "freq", "gain", "q", "on"))
                clean[key].append({"type": choice(band["type"], ("bell", "low_shelf", "high_shelf", "low_cut", "high_cut")),
                                   "freq": number(band["freq"], 5, 20000), "gain": number(band["gain"], -18, 18),
                                   "q": number(band["q"], 0.1, 18), "on": boolean(band["on"])})
        elif key == "modules":
            object_fields(item, MODULE_PARAMETERS)
            modules = clean[key] = {}
            for module, parameters in item.items():
                object_fields(parameters, {"enabled", *MODULE_PARAMETERS[module]})
                for count in ("max_notches", "max_run"):
                    if count in parameters:
                        entry = number(parameters[count], *MODULE_PARAMETERS[module][count])
                        if entry != int(entry):
                            raise ApiError(400, "invalid_module_count")
                modules[module] = {name: boolean(entry) if name == "enabled" else
                                   number(entry, *MODULE_PARAMETERS[module][name]) for name, entry in parameters.items()}
    return clean
