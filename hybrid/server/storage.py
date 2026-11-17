"""Exclusive ephemeral storage, marked ownership, and bounded disk accounting."""

import json
import os
from pathlib import Path
import re
import shutil
import stat

from .errors import ApiError, ProcessingError

ROOT_MARKER = ".slimeybeats-hybrid-root"
JOB_MARKER = ".slimeybeats-hybrid-job"
OWNER = "slimeybeats-hybrid-private-v1"
JOB_ID = re.compile(r"[a-f0-9]{32}\Z")


def is_link(path):
    return path.is_symlink() or (hasattr(path, "is_junction") and path.is_junction())


def directory_usage(path, limit=None, max_entries=128):
    total = 0
    count = 0
    pending = [path]
    while pending:
        folder = pending.pop()
        if is_link(folder):
            raise ProcessingError("Unsafe storage entry")
        try:
            entries = os.scandir(folder)
        except FileNotFoundError:
            if folder == path:
                raise
            # Native tools remove their own private temporary directories while
            # this watchdog walks the project. A vanished descendant uses no
            # disk; the root, links, other I/O errors and file limits still fail.
            continue
        with entries:
            for entry in entries:
                count += 1
                if count > max_entries:
                    raise ProcessingError("Too many processing files")
                child = Path(entry.path)
                try:
                    if is_link(child):
                        raise ProcessingError("Unsafe storage entry")
                    details = entry.stat(follow_symlinks=False)
                    if stat.S_ISDIR(details.st_mode):
                        pending.append(child)
                    elif stat.S_ISREG(details.st_mode):
                        total += details.st_size
                    else:
                        raise ProcessingError("Unsafe storage entry")
                except FileNotFoundError:
                    continue
                if limit is not None and total > limit:
                    raise ProcessingError("Processing storage limit exceeded")
    return total, count


def directory_bytes(path, limit=None, max_entries=128):
    return directory_usage(path, limit, max_entries)[0]


class Storage:
    def __init__(self, config):
        self.config = config
        self.root = config.data_dir
        self.lock_file = None
        if is_link(self.root):
            raise ValueError("Storage root must not be a link")
        self.root.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.root = self.root.resolve(strict=True)
        marker = self.root / ROOT_MARKER
        if not marker.exists():
            if any(self.root.iterdir()):
                raise ValueError("Storage root is not an empty or owned hybrid directory")
            marker.write_text(OWNER, encoding="ascii")
        if is_link(marker) or marker.read_text(encoding="ascii") != OWNER:
            raise ValueError("Invalid hybrid storage marker")
        if os.name == "posix":
            self.root.chmod(0o700)
        try:
            self._lock()
            # Old jobs are never resumed: upload bytes and outputs have no durable contract.
            for directory in self.root.iterdir():
                if directory.name.startswith("job-") and JOB_ID.fullmatch(directory.name[4:]):
                    if self.owns(directory, directory.name[4:]):
                        self.remove(directory, directory.name[4:])
        except BaseException:
            self.close()
            raise

    def _lock(self):
        path = self.root / ".server.lock"
        if is_link(path):
            raise ValueError("Unsafe storage lock")
        self.lock_file = path.open("a+b")
        self.lock_file.seek(0)
        if os.name == "nt":
            import msvcrt
            if path.stat().st_size == 0:
                self.lock_file.write(b"0")
                self.lock_file.flush()
            self.lock_file.seek(0)
            msvcrt.locking(self.lock_file.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(self.lock_file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)

    def owns(self, directory, job_id):
        marker = directory / JOB_MARKER
        if (directory.parent != self.root or directory.name != "job-" + job_id
                or is_link(directory) or not directory.is_dir() or is_link(marker)
                or not marker.is_file() or marker.stat().st_size > 256):
            return False
        try:
            return json.loads(marker.read_text(encoding="ascii")) == {"owner": OWNER, "id": job_id}
        except (ValueError, OSError, UnicodeError):
            return False

    def create(self, job_id):
        assert JOB_ID.fullmatch(job_id)
        directory = self.root / ("job-" + job_id)
        directory.mkdir(mode=0o700)
        try:
            (directory / JOB_MARKER).write_text(json.dumps({"owner": OWNER, "id": job_id}), encoding="ascii")
        except OSError:
            directory.rmdir()
            raise
        return directory

    def remove(self, directory, job_id):
        if not self.owns(directory, job_id):
            raise ProcessingError("Refused to delete unowned storage")
        # rmtree unlinks symlinks/junctions rather than traversing their destinations.
        shutil.rmtree(directory)

    def check_space(self, jobs, new_reservation=False, reservation_bytes=0):
        reserved = 0
        for job in jobs:
            budget = getattr(job, "max_disk_bytes", self.config.max_job_disk_bytes)
            used = directory_bytes(job.directory, budget, getattr(job, "max_storage_files", 128))
            reserved += max(0, budget - used)
        if new_reservation:
            reserved += self.config.max_job_disk_bytes
        reserved += reservation_bytes
        if shutil.disk_usage(self.root).free < reserved + self.config.min_free_bytes:
            raise ApiError(507, "Temporary processing storage is full")

    def close(self):
        if self.lock_file is not None:
            self.lock_file.close()
            self.lock_file = None
