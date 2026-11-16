"""Bounded version-one API with explicit owner and optional guest principals."""

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import logging
import re
import socket
import threading
from urllib.parse import parse_qs, urlsplit

from .errors import ApiError
from .client_identity import check_host, client_key
from .http_auth import ALLOW_HEADERS, ALLOW_METHODS, allowed_origin, authenticate, preflight
from .http_deadline import DeadlineReader

LOGGER = logging.getLogger("slimeybeats.hybrid")
JOB_ROUTE = re.compile(r"/v1/jobs/([a-f0-9]{32})(?:/(upload|run|artifacts/([a-f0-9]{32})))?\Z")


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("Duplicate JSON field")
        result[key] = value
    return result


def invalid_constant(value):
    raise ValueError("Nonfinite JSON constant")


class HybridHTTPServer(ThreadingHTTPServer):
    daemon_threads = False
    allow_reuse_address = True
    request_queue_size = 16

    def __init__(self, config, store):
        self.config = config
        self.store = store
        self.slots = threading.BoundedSemaphore(config.max_connections)
        super().__init__((config.host, config.port), Handler)

    def get_request(self):
        connection, address = super().get_request()
        connection.settimeout(self.config.request_timeout_seconds)
        return connection, address

    def process_request(self, request, client_address):
        if not self.slots.acquire(blocking=False):
            try:
                request.sendall(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            except OSError:
                pass
            self.shutdown_request(request)
            return
        try:
            super().process_request(request, client_address)
        except BaseException:
            self.slots.release()
            raise

    def process_request_thread(self, request, client_address):
        try:
            super().process_request_thread(request, client_address)
        finally:
            self.slots.release()

    def handle_error(self, request, client_address):
        LOGGER.error("Hybrid request handler failed")


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def setup(self):
        super().setup()
        self.rfile = DeadlineReader(self.rfile, self.connection, self.server.config.request_timeout_seconds)

    def version_string(self):
        return "SlimeyBeatsHybrid"

    def log_message(self, format, *args):
        # Even rejected paths, headers, filenames and query strings are private.
        pass

    def send_error(self, code, message=None, explain=None):
        self.json_response(code, {"error": "Invalid HTTP request"})

    def handle_expect_100(self):
        self.json_response(417, {"error": "Expect is not supported"})
        return False

    def headers_for(self, status, length, mime="application/json", filename=None, cors=False, retry_after=None, extra_headers=None):
        self.connection.settimeout(self.server.config.request_timeout_seconds)
        self.send_response(status)
        self.send_header("Content-Type", mime)
        self.send_header("Content-Length", str(length))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Referrer-Policy", "no-referrer")
        self.send_header("Connection", "close")
        self.close_connection = True
        self.send_header("Vary", "Origin")
        origin = self.headers.get("Origin") if hasattr(self, "headers") else None
        if origin in self.server.config.origins:
            self.send_header("Access-Control-Allow-Origin", origin)
            self.send_header("Access-Control-Expose-Headers", "Content-Disposition, Retry-After, Accept-Ranges, Content-Range, Content-Length")
            if cors:
                self.send_header("Access-Control-Allow-Methods", ALLOW_METHODS)
                self.send_header("Access-Control-Allow-Headers", ALLOW_HEADERS)
                self.send_header("Access-Control-Max-Age", "600")
        if status == 401:
            self.send_header("WWW-Authenticate", "Bearer")
        if filename:
            self.send_header("Content-Disposition", 'attachment; filename="' + filename + '"')
        if retry_after is not None:
            self.send_header("Retry-After", str(retry_after))
        for name, value in (extra_headers or {}).items():
            self.send_header(name, value)
        self.end_headers()

    def json_response(self, status, value, cors=False, retry_after=None):
        data = b"" if status == 204 else json.dumps(value, allow_nan=False, separators=(",", ":")).encode("utf-8")
        self.headers_for(status, len(data), cors=cors, retry_after=retry_after)
        self.wfile.write(data)

    def body_length(self, maximum):
        if self.headers.get_all("Transfer-Encoding"):
            raise ApiError(400, "Transfer-Encoding is not supported")
        lengths = self.headers.get_all("Content-Length", [])
        if len(lengths) != 1 or not re.fullmatch(r"[0-9]{1,12}", lengths[0]):
            raise ApiError(411, "One valid Content-Length is required")
        length = int(lengths[0])
        if length > maximum:
            raise ApiError(413, "Request body exceeds the byte limit")
        return length

    def body(self, maximum, json_body=False):
        length = self.body_length(maximum)
        if json_body and self.headers.get("Content-Type", "").split(";", 1)[0].strip().lower() != "application/json":
            raise ApiError(415, "Content-Type must be application/json")
        data = self.rfile.read(length)
        if len(data) != length:
            raise ApiError(400, "Incomplete request body")
        if not json_body:
            return data
        try:
            return json.loads(data.decode("utf-8"), object_pairs_hook=unique_object,
                              parse_constant=invalid_constant)
        except (ValueError, UnicodeError, RecursionError) as error:
            raise ApiError(400, "Invalid JSON request") from error

    def dispatch(self):
        try:
            check_host(self)
            allowed_origin(self)
            if (not self.path.startswith("/") or self.path.startswith("//")
                    or self.requestline.split()[1] != self.path):
                raise ApiError(400, "Invalid request target")
            parsed = urlsplit(self.path)
            if parsed.fragment or parsed.scheme or parsed.netloc:
                raise ApiError(400, "Invalid request target")
            path = parsed.path
            if self.command == "OPTIONS":
                if not path.startswith(("/v1/", "/v2/")):
                    raise ApiError(404, "Endpoint not found")
                preflight(self)
                self.json_response(204, None, cors=True)
                return
            if path == "/health" and not parsed.query and self.command == "GET":
                self.json_response(200, {"status": "up"})
                return
            if self.path == "/v1/status" and self.command == "GET":
                self.json_response(200, self.server.store.public_status())
                return
            if self.path == "/v1/session" and self.command == "POST":
                if not self.server.config.public_enabled:
                    raise ApiError(403, "Public guest access is disabled")
                if not allowed_origin(self):
                    raise ApiError(403, "Guest sessions require a configured web origin")
                key = client_key(self)
                if self.body(16384, json_body=True) != {}:
                    raise ApiError(400, "Session accepts an empty JSON object")
                self.json_response(201, self.server.store.sessions.mint(key))
                return
            if path == "/v2" or path.startswith("/v2/"):
                from .project_http import dispatch
                dispatch(self, parsed, authenticate(self))
                return
            if path == "/v1" or path.startswith("/v1/"):
                principal = authenticate(self)
            else:
                raise ApiError(404, "Endpoint not found")
            if path == "/v1/capabilities" and not parsed.query and self.command == "GET":
                self.json_response(200, self.server.config.capabilities())
                return
            if path == "/v1/jobs" and not parsed.query and self.command == "POST":
                keys = self.headers.get_all("Idempotency-Key", [])
                if len(keys) > 1:
                    raise ApiError(400, "Only one Idempotency-Key is accepted")
                value = self.body(16384, json_body=True)
                self.json_response(201, self.server.store.create(value, keys[0] if keys else None,
                                                                principal=principal))
                return
            match = JOB_ROUTE.fullmatch(path)
            if not match:
                raise ApiError(404, "Endpoint not found")
            job_id, action, artifact_id = match.groups()
            store = self.server.store
            store.snapshot(job_id, principal=principal)
            if action == "upload" and self.command == "PUT":
                query = parse_qs(parsed.query, keep_blank_values=True)
                if set(query) != {"offset"} or len(query["offset"]) != 1 or not re.fullmatch(r"[0-9]{1,12}", query["offset"][0]):
                    raise ApiError(400, "One nonnegative upload offset is required")
                value = store.upload(job_id, int(query["offset"][0]), self.body(self.server.config.chunk_bytes),
                                     principal=principal)
            elif parsed.query:
                raise ApiError(400, "Query parameters are not accepted")
            elif action == "run" and self.command == "POST":
                if self.body(16384, json_body=True) != {}:
                    raise ApiError(400, "Run accepts an empty JSON object")
                value = store.run(job_id, principal=principal,
                                  client_key=client_key(self) if not principal.is_owner else None)
            elif action is None and self.command == "GET":
                value = store.snapshot(job_id, principal=principal)
            elif action is None and self.command == "DELETE":
                value = store.delete(job_id, principal=principal)
            elif artifact_id and self.command == "GET":
                with store.artifact(job_id, artifact_id, principal=principal) as (artifact, stream):
                    self.headers_for(200, artifact["bytes"], artifact["content_type"], artifact["name"])
                    while block := stream.read(65536):
                        if not store.download_current(job_id, principal=principal):
                            self.close_connection = True
                            return
                        self.wfile.write(block)
                return
            else:
                raise ApiError(405, "Method is not supported")
            self.json_response(200, value)
        except ApiError as error:
            message = error.message
            if self.path.startswith("/v2/") and not re.fullmatch(r"[a-z_]{1,64}", message):
                message = {400: "invalid_request", 401: "authentication_required", 403: "access_denied",
                           404: "not_found", 409: "conflict", 413: "request_limit", 429: "rate_limit",
                           507: "storage_full"}.get(error.status, "request_failed")
            self.json_response(error.status, {"error": message}, retry_after=error.retry_after)
        except (BrokenPipeError, ConnectionResetError, socket.timeout):
            self.close_connection = True
        except Exception:
            LOGGER.error("Hybrid request failed")
            self.json_response(500, {"error": "Request could not be completed"})

    do_GET = dispatch
    do_POST = dispatch
    do_PUT = dispatch
    do_DELETE = dispatch
    do_OPTIONS = dispatch
    do_HEAD = dispatch
    do_PATCH = dispatch
