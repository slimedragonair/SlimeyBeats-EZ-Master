"""Bounded authenticated probes, with no redirects, proxy inheritance or bodies logged."""

from __future__ import annotations

import json
import http.client
import urllib.error
import urllib.parse
import urllib.request

PAGES_ORIGIN = "https://slimedragonair.github.io"
MAX_RESPONSE_BYTES = 64 * 1024
EXPECTED_LIMITS = {
    "max_upload_bytes": 512 * 1024**2,
    "chunk_bytes": 1024**2,
    "max_jobs": 4,
    "retention_seconds": 3600,
}


class _NoRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def _endpoint_allowed(endpoint: str) -> bool:
    try:
        parsed = urllib.parse.urlsplit(endpoint)
        if parsed.username or parsed.password or parsed.query or parsed.fragment:
            return False
        if parsed.path not in ("", "/"):
            return False
        return (
            parsed.scheme == "https" and parsed.hostname == "api.slimedragon.net"
            and parsed.port in (None, 443)
        ) or (
            parsed.scheme == "http" and parsed.hostname == "127.0.0.1"
            and parsed.port is not None and 0 < parsed.port < 65536
        )
    except ValueError:
        return False


def _request(opener, endpoint, path, token="", origin=PAGES_ORIGIN, method="GET"):
    headers = {"Origin": origin, "Accept": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    if method == "OPTIONS":
        headers.update({
            "Access-Control-Request-Method": "PUT",
            "Access-Control-Request-Headers": "authorization,content-type",
        })
    request = urllib.request.Request(endpoint.rstrip("/") + path, headers=headers, method=method)
    try:
        with opener.open(request, timeout=10) as response:
            body = response.read(MAX_RESPONSE_BYTES + 1)
            if len(body) > MAX_RESPONSE_BYTES:
                return response.status, response.headers, None
            try:
                document = json.loads(body) if body else None
            except (ValueError, UnicodeDecodeError):
                document = None
            return response.status, response.headers, document
    except urllib.error.HTTPError as error:
        # Do not read or print error bodies, redirect locations, or exceptions.
        error.close()
        return error.code, error.headers, None
    except (OSError, urllib.error.URLError, ValueError, http.client.HTTPException):
        return 0, {}, None


def check_health(endpoint: str, token: str) -> dict[str, bool]:
    checks = {
        "approved_endpoint": _endpoint_allowed(endpoint),
        "token_at_least_32_characters": len(token) >= 32,
        "token_header_safe": all(32 < ord(char) < 127 for char in token),
    }
    if not all(checks.values()):
        return checks
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), _NoRedirects())
    status, _, body = _request(opener, endpoint, "/health")
    checks["health_json_up"] = status == 200 and isinstance(body, dict)
    status, _, _ = _request(opener, endpoint, "/v1/capabilities")
    checks["capabilities_requires_auth"] = status in (401, 403)
    status, headers, body = _request(opener, endpoint, "/v1/capabilities", token)
    body = body if isinstance(body, dict) else {}
    operations, formats, profiles = (body.get(key) for key in ("operations", "formats", "profiles"))
    limits, privacy = (body.get(key) for key in ("limits", "privacy"))
    checks.update({
        "authenticated_capabilities": status == 200,
        "api_version_1": type(body.get("api_version")) is int and body["api_version"] == 1,
        "master_operation": isinstance(operations, list) and "master" in operations,
        "wav24_and_mp3": isinstance(formats, list)
        and all(isinstance(item, str) for item in formats) and set(formats) == {"wav24", "mp3"},
        "profiles_available": isinstance(profiles, list) and bool(profiles)
        and all(isinstance(item, dict) and isinstance(item.get("id"), str)
                and isinstance(item.get("label"), str) for item in profiles),
        "preview_limits_match": isinstance(limits, dict)
        and all(type(limits.get(key)) is int and limits[key] == value for key, value in EXPECTED_LIMITS.items()),
        "temporary_one_hour_storage": isinstance(privacy, dict)
        and privacy.get("temporary_storage") is True
        and type(privacy.get("retention_seconds")) is int and privacy["retention_seconds"] == 3600,
        "exact_cors_origin": headers.get("Access-Control-Allow-Origin") == PAGES_ORIGIN,
        "no_credentialed_cors": headers.get("Access-Control-Allow-Credentials", "").lower() != "true",
    })
    status, headers, _ = _request(opener, endpoint, "/v1/jobs/example/upload", method="OPTIONS")
    allowed_headers = {part.strip().lower() for part in headers.get("Access-Control-Allow-Headers", "").split(",")}
    checks["upload_preflight"] = (
        status in (200, 204) and headers.get("Access-Control-Allow-Origin") == PAGES_ORIGIN
        and {"authorization", "content-type"} <= allowed_headers
        and "PUT" in headers.get("Access-Control-Allow-Methods", "").upper()
    )
    status, headers, _ = _request(opener, endpoint, "/v1/capabilities", origin="https://untrusted.invalid", method="OPTIONS")
    checks["unapproved_origin_rejected"] = (
        status != 0 and not 300 <= status < 400
        and headers.get("Access-Control-Allow-Origin") is None
    )
    return checks
