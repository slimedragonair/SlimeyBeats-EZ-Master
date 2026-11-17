"""Shared model IDs are distinct from server-generated storage/upload tokens."""
from .errors import ApiError


def entity_id(value):
    try:
        valid = isinstance(value, str) and 1 <= len(value.encode('utf-8')) <= 128 and not any(
            ord(char) < 32 or char in '/\\' for char in value)
    except UnicodeError:
        valid = False
    if not valid:
        raise ApiError(400, 'invalid_arrangement_entity_id')
