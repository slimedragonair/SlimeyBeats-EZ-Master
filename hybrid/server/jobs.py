"""Locked job state, upload reservations, immutable queue entries and deletion."""

from collections import deque
from contextlib import contextmanager
from dataclasses import dataclass, field
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import stat
import threading
import time

from .errors import ApiError, ProcessingError
from .requests import parse_master_request
from .storage import is_link


@dataclass
class Job:
    id: str
    request: object
    directory: Path
    expires: float
    deadline: float
    state: str = "uploading"
    uploaded: int = 0
    progress: float = 0
    message: str = "Ready for upload"
    artifacts: dict = field(default_factory=dict)
    cancel: threading.Event = field(default_factory=threading.Event)
    done: threading.Event = field(default_factory=threading.Event)
    downloads_done: threading.Event = field(default_factory=threading.Event)
    downloads: int = 0
    unreaped: bool = False

    def __post_init__(self):
        self.done.set()
        self.downloads_done.set()

    def snapshot(self):
        return {"id": self.id, "state": self.state, "uploaded_bytes": self.uploaded,
                "total_bytes": self.request.total_bytes, "progress": self.progress,
                "message": self.message,
                "expires_at": datetime.fromtimestamp(self.expires, timezone.utc).isoformat().replace("+00:00", "Z"),
                "artifacts": [{key: value for key, value in artifact.items() if key != "path"}
                              for artifact in self.artifacts.values()]}


class JobStore:
    def __init__(self, config, storage):
        self.config = config
        self.storage = storage
        self.lock = threading.RLock()
        self.changed = threading.Condition(self.lock)
        self.jobs = {}
        self.queue = deque()
        self.idempotency = {}
        self.stopping = False

    def _get(self, job_id):
        job = self.jobs.get(job_id)
        if job is None or job.deadline <= time.monotonic():
            raise ApiError(404, "Job not found or expired")
        return job

    def create(self, value, idempotency_key=None):
        request = parse_master_request(value, self.config)
        fingerprint = hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                               allow_nan=False).encode()).hexdigest()
        if idempotency_key is not None and not re.fullmatch(r"[A-Za-z0-9_-]{16,128}", idempotency_key):
            raise ApiError(400, "Invalid Idempotency-Key")
        with self.lock:
            if self.stopping:
                raise ApiError(503, "Service is stopping")
            if idempotency_key in self.idempotency:
                previous_id, previous_fingerprint = self.idempotency[idempotency_key]
                if fingerprint != previous_fingerprint:
                    raise ApiError(409, "Idempotency-Key was used for different job fields")
                return self._get(previous_id).snapshot()
            if len(self.jobs) >= self.config.max_jobs:
                raise ApiError(429, "Temporary job limit reached; delete a job or wait for expiry")
            if sum(j.request.total_bytes for j in self.jobs.values()) + request.total_bytes > self.config.max_jobs * self.config.max_upload_bytes:
                raise ApiError(413, "Upload reservation limit reached")
            self.storage.check_space(self.jobs.values(), new_reservation=True)
            job_id = secrets.token_hex(16)
            directory = self.storage.create(job_id)
            job = Job(job_id, request, directory, time.time() + self.config.retention_seconds,
                      time.monotonic() + self.config.retention_seconds)
            self.jobs[job_id] = job
            if idempotency_key is not None:
                self.idempotency[idempotency_key] = (job_id, fingerprint)
            return job.snapshot()

    def snapshot(self, job_id):
        with self.lock:
            return self._get(job_id).snapshot()

    def upload(self, job_id, offset, payload):
        with self.lock:
            job = self._get(job_id)
            if job.state != "uploading":
                raise ApiError(409, "Job no longer accepts uploads")
            if offset != job.uploaded:
                raise ApiError(409, "Upload offset does not match uploaded_bytes")
            if not payload or len(payload) > self.config.chunk_bytes or job.uploaded + len(payload) > job.request.total_bytes:
                raise ApiError(413, "Chunk exceeds upload bounds")
            self.storage.check_space(self.jobs.values())
            path = job.directory / ("upload." + job.request.extension)
            if is_link(path):
                raise ApiError(500, "Temporary upload is unavailable")
            try:
                with path.open("a+b") as output:
                    if output.tell() != job.uploaded:
                        raise ApiError(409, "Upload storage does not match uploaded_bytes")
                    try:
                        if output.write(payload) != len(payload):
                            raise OSError("Short upload write")
                        output.flush()
                    except OSError:
                        output.truncate(job.uploaded)
                        raise
            except OSError as error:
                raise ApiError(507, "Upload storage write failed") from error
            job.uploaded += len(payload)
            job.progress = 0.25 * job.uploaded / job.request.total_bytes
            job.message = "Upload complete" if job.uploaded == job.request.total_bytes else "Uploading"
            return job.snapshot()

    def run(self, job_id):
        with self.changed:
            job = self._get(job_id)
            if job.state != "uploading":
                return job.snapshot()
            if job.uploaded != job.request.total_bytes:
                raise ApiError(409, "Upload is incomplete")
            job.state = "queued"
            job.progress = 0.3
            job.message = "Queued for processing"
            self.queue.append(job_id)
            self.changed.notify()
            return job.snapshot()

    def claim(self):
        with self.changed:
            while not self.stopping:
                while self.queue:
                    job_id = self.queue.popleft()
                    job = self.jobs.get(job_id)
                    if (job is not None and job.state == "queued" and not job.cancel.is_set()
                            and job.deadline > time.monotonic()):
                        job.state = "running"
                        job.progress = 0.5
                        job.message = "Processing audio"
                        job.done.clear()
                        return job
                self.changed.wait(timeout=0.25)
        return None

    def delete(self, job_id, expired=False):
        with self.lock:
            job = self.jobs.get(job_id) if expired else self._get(job_id)
            if job is None:
                return {"deleted": True}
            job.cancel.set()
            job.state = "cancelled"
            job.message = "Cancelled"
        if not job.done.wait(timeout=30) or not job.downloads_done.wait(timeout=30) or job.unreaped:
            raise ApiError(503, "Owned processing or downloads are still stopping; retry deletion")
        with self.lock:
            if self.jobs.get(job_id) is job:
                try:
                    self.storage.remove(job.directory, job_id)
                except (OSError, ProcessingError) as error:
                    raise ApiError(503, "Temporary files could not be removed; retry deletion") from error
                del self.jobs[job_id]
                self.queue = deque(item for item in self.queue if item != job_id)
                self.idempotency = {key: item for key, item in self.idempotency.items() if item[0] != job_id}
        return {"deleted": True}

    @contextmanager
    def artifact(self, job_id, artifact_id):
        with self.lock:
            job = self._get(job_id)
            artifact = job.artifacts.get(artifact_id)
            if job.state != "completed" or artifact is None:
                raise ApiError(404, "Artifact not found")
            path = artifact["path"]
            if is_link(path) or path.parent != job.directory / "out" or is_link(path.parent):
                raise ApiError(404, "Artifact not found")
            try:
                descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0))
                stream = os.fdopen(descriptor, "rb")
                details = os.fstat(stream.fileno())
                if not stat.S_ISREG(details.st_mode) or details.st_size != artifact["bytes"] or details.st_nlink != 1:
                    stream.close()
                    raise OSError("Artifact changed")
            except OSError as error:
                raise ApiError(404, "Artifact not found") from error
            job.downloads += 1
            job.downloads_done.clear()
        try:
            yield artifact, stream
        finally:
            stream.close()
            with self.lock:
                job.downloads -= 1
                if job.downloads == 0:
                    job.downloads_done.set()

    def download_current(self, job_id):
        with self.lock:
            job = self.jobs.get(job_id)
            return (job is not None and job.state == "completed" and not job.cancel.is_set()
                    and job.deadline > time.monotonic())

    def expire(self):
        with self.lock:
            expired = [job.id for job in self.jobs.values() if job.deadline <= time.monotonic()]
        failure = None
        for job_id in expired:
            try:
                self.delete(job_id, expired=True)
            except ApiError as error:
                failure = error
        if failure is not None:
            raise failure

    def stop(self):
        with self.changed:
            self.stopping = True
            for job in self.jobs.values():
                job.cancel.set()
            self.changed.notify_all()
