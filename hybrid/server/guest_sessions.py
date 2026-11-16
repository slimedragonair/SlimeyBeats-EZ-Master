"""Opaque, fixed-lifetime guest sessions and bounded, non-evicting rate windows."""

from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import hmac
import math
import re
import secrets
import threading
import time

from .errors import ApiError
from .principals import Principal

MAX_SESSIONS = 64
MAX_RATE_ENTRIES = 256
SESSION_WINDOW_SECONDS = 600
SESSIONS_PER_IP = 4
SESSIONS_PER_WINDOW = 64
JOB_WINDOW_SECONDS = 3600
JOBS_PER_IP = 8
JOBS_PER_SESSION = 3
JOBS_PER_WINDOW = 32
TOKEN_PATTERN = re.compile(r"[A-Za-z0-9_-]{43}\Z")


@dataclass
class Window:
    deadline: float
    count: int = 0


@dataclass
class Session:
    principal: Principal
    job_starts: int = 0
    command_starts: int = 0


def limited(message, deadline, now):
    return ApiError(429, message, retry_after=max(1, math.ceil(deadline - now)))


class GuestRegistry:
    def __init__(self, config, monotonic=time.monotonic, wall_time=time.time):
        self.config = config
        self.monotonic = monotonic
        self.wall_time = wall_time
        self.lock = threading.RLock()
        self._nonce = secrets.token_bytes(32)
        self._sessions = {}  # SHA-256 bearer digest -> session; never retain bearer text.
        self._rates = {}  # (operation, nonce-hashed address) -> watched window.
        self._totals = {}  # At most two fixed operation keys.

    def client_key(self, address):
        return hmac.digest(self._nonce, address.encode("ascii"), "sha256")

    def _purge(self, now):
        self._sessions = {key: value for key, value in self._sessions.items()
                          if value.principal.deadline > now}
        self._rates = {key: value for key, value in self._rates.items() if value.deadline > now}
        self._totals = {key: value for key, value in self._totals.items() if value.deadline > now}

    def expire(self):
        with self.lock:
            self._purge(self.monotonic())

    def _consume(self, operation, client_key, per_ip, total_limit, seconds, now):
        key = (operation, client_key)
        local = self._rates.get(key)
        total = self._totals.get(operation)
        if local is not None and local.count >= per_ip:
            raise limited("Guest request rate limit reached", local.deadline, now)
        if total is not None and total.count >= total_limit:
            raise limited("Public request rate limit reached", total.deadline, now)
        # Never evict a live window: rotating addresses must not reset throttles.
        if local is None and len(self._rates) >= MAX_RATE_ENTRIES:
            raise limited("Guest rate tracking capacity reached",
                          min(item.deadline for item in self._rates.values()), now)
        if local is None:
            local = self._rates[key] = Window(now + seconds)
        if total is None:
            total = self._totals[operation] = Window(now + seconds)
        local.count += 1
        total.count += 1

    def mint(self, client_key):
        with self.lock:
            if not self.config.public_enabled:
                raise ApiError(403, "Public guest access is disabled")
            now = self.monotonic()
            self._purge(now)
            if len(self._sessions) >= MAX_SESSIONS:
                raise limited("Guest session capacity reached",
                              min(item.principal.deadline for item in self._sessions.values()), now)
            self._consume("session", client_key, SESSIONS_PER_IP, SESSIONS_PER_WINDOW,
                          SESSION_WINDOW_SECONDS, now)
            token = secrets.token_urlsafe(32)
            principal = Principal(secrets.token_hex(16),
                                  self.wall_time() + self.config.guest_session_ttl_seconds,
                                  now + self.config.guest_session_ttl_seconds)
            self._sessions[hashlib.sha256(token.encode("ascii")).digest()] = Session(principal)
            return {"token": token, "expires_at": datetime.fromtimestamp(
                principal.expires, timezone.utc).isoformat().replace("+00:00", "Z")}

    def authenticate(self, token):
        with self.lock:
            now = self.monotonic()
            self._purge(now)
            if not self.config.public_enabled or TOKEN_PATTERN.fullmatch(token) is None:
                raise ApiError(401, "Bearer authentication is required")
            session = self._sessions.get(hashlib.sha256(token.encode("ascii")).digest())
            if session is None:
                raise ApiError(401, "Bearer authentication is required")
            return session.principal

    def _session(self, principal, now):
        if self.config.public_enabled and principal.deadline > now:
            for session in self._sessions.values():
                if session.principal == principal:
                    return session
        raise ApiError(401, "Bearer authentication is required")

    def validate(self, principal):
        if not principal.is_owner:
            with self.lock:
                self._session(principal, self.monotonic())

    def start_job(self, principal, client_key):
        with self.lock:
            now = self.monotonic()
            self._purge(now)
            session = self._session(principal, now)
            if session.job_starts >= JOBS_PER_SESSION:
                raise limited("Guest job rate limit reached", principal.deadline, now)
            self._consume("job", client_key, JOBS_PER_IP, JOBS_PER_WINDOW, JOB_WINDOW_SECONDS, now)
            session.job_starts += 1

    def start_command(self, principal, client_key):
        with self.lock:
            now = self.monotonic()
            self._purge(now)
            session = self._session(principal, now)
            if session.command_starts >= 120:
                raise limited("command_quota", principal.deadline, now)
            self._consume("command", client_key, 480, 1920, JOB_WINDOW_SECONDS, now)
            session.command_starts += 1
