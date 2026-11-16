"""Allow only bounded expressions of built-in profiles before native resolution."""

import math
import re

from .config import PROFILES
from .errors import ApiError

WEIGHT = re.compile(r"(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?\Z")


def builtin_blend(value):
    if not isinstance(value, str) or not 1 <= len(value) <= 128:
        raise ApiError(400, "Invalid built-in profile blend")
    parts = re.split(r"(?<![eE])\+", value)
    if len(parts) > 2:
        raise ApiError(400, "At most two built-in profiles are supported")
    active = []
    for part in parts:
        pieces = part.split(":")
        if len(pieces) > 2 or pieces[0] not in PROFILES:
            raise ApiError(400, "Unknown built-in profile")
        weight = 1.0
        if len(pieces) == 2:
            if not WEIGHT.fullmatch(pieces[1]):
                raise ApiError(400, "Invalid profile weight")
            weight = float(pieces[1])
        if not math.isfinite(weight) or not 0 <= weight <= 1:
            raise ApiError(400, "Profile weights must be finite and between zero and one")
        if weight > 0:
            active.append((pieces[0], weight))
    if not active:
        raise ApiError(400, "Profile weights must have a positive total")
    # The native resolver requires positive weights and recognizes weights only
    # in multi-component expressions. Remove zeroes and redundant single weights.
    if len(active) == 1:
        return active[0][0]
    return "+".join(f"{key}:{weight!r}" for key, weight in active)
