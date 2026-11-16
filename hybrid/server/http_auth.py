"""Exact CORS origins and constant-time bearer checks, without cookie credentials."""

import hmac

from .errors import ApiError

ALLOW_HEADERS = "Authorization, Content-Type, Idempotency-Key"
ALLOW_METHODS = "GET, POST, PUT, DELETE, OPTIONS"


def allowed_origin(handler):
    origins = handler.headers.get_all("Origin", [])
    if not origins:
        return None
    if len(origins) != 1 or origins[0] not in handler.server.config.origins:
        raise ApiError(403, "Web origin is not allowed")
    return origins[0]


def authenticate(handler):
    values = handler.headers.get_all("Authorization", [])
    supplied = values[0] if len(values) == 1 else ""
    expected = "Bearer " + handler.server.config.token
    if not hmac.compare_digest(supplied.encode("utf-8"), expected.encode("ascii")):
        raise ApiError(401, "Bearer authentication is required")


def preflight(handler):
    if not allowed_origin(handler):
        raise ApiError(403, "Preflight requires a configured web origin")
    if handler.headers.get("Access-Control-Request-Method") not in ALLOW_METHODS.split(", "):
        raise ApiError(403, "Preflight method is not allowed")
    requested = {item.strip().lower() for item in handler.headers.get("Access-Control-Request-Headers", "").split(",") if item.strip()}
    if not requested <= {"authorization", "content-type", "idempotency-key"}:
        raise ApiError(403, "Preflight headers are not allowed")
