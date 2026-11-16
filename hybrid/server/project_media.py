"""Authenticated media leases and single-range streaming without whole-file reads."""

from contextlib import contextmanager
import os
import re
import stat

from .errors import ApiError, ProcessingError
from .principals import OWNER
from .studio_driver import safe_file

RANGE = re.compile(r"bytes=([0-9]{0,20})-([0-9]{0,20})\Z")


def byte_range(header, size):
    if header is None:
        return 0, size - 1, False
    match = RANGE.fullmatch(header)
    if not match or not any(match.groups()) or size <= 0:
        raise ValueError("Invalid Range")
    first, last = match.groups()
    if first:
        start, end = int(first), int(last) if last else size - 1
        if start >= size or end < start:
            raise ValueError("Unsatisfiable Range")
        return start, min(end, size - 1), True
    suffix = int(last)
    if suffix <= 0:
        raise ValueError("Unsatisfiable Range")
    return max(0, size - suffix), size - 1, True


@contextmanager
def lease(store, project_id, media_id, kind, principal=OWNER):
    with store.lock:
        project = store.get(project_id, principal)
        if kind == "artifact":
            entry = project.artifacts.get(media_id)
            if entry is None:
                raise ApiError(404, "artifact_not_found")
            path, name, mime, size = entry["path"], entry["name"], entry["content_type"], entry["bytes"]
        else:
            entry = project.assets.get(media_id)
            if entry is None:
                raise ApiError(404, "audio_not_found")
            key = {"audio": "path", "preview": "preview_path", "waveform": "waveform_path"}[kind]
            path = entry.get(key)
            if path is None:
                raise ApiError(404, "media_not_found")
            mime = "audio/mpeg" if kind == "preview" else "application/json" if kind == "waveform" else entry["content_type"]
            name = entry.get("kind", entry["role"]) + (".mp3" if kind == "preview" else ".json" if kind == "waveform" else ".wav")
            size = entry["bytes"] if kind == "audio" else None
        try:
            path = safe_file(path, project.directory)
            descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0))
            stream = os.fdopen(descriptor, "rb")
            details = os.fstat(stream.fileno())
            if not stat.S_ISREG(details.st_mode) or details.st_nlink != 1 or size is not None and size != details.st_size:
                stream.close()
                raise OSError("Media changed")
        except (OSError, ProcessingError) as error:
            raise ApiError(404, "media_not_found") from error
        project.downloads += 1
        project.downloads_done.clear()
    try:
        yield {"bytes": details.st_size, "name": name, "content_type": mime}, stream
    finally:
        stream.close()
        with store.lock:
            project.downloads -= 1
            if not project.downloads:
                project.downloads_done.set()


def current(store, project_id, principal):
    with store.lock:
        try:
            store.get(project_id, principal)
            return True
        except ApiError:
            return False


def stream_media(handler, store, project_id, media_id, kind, principal):
    with lease(store, project_id, media_id, kind, principal) as (metadata, stream):
        size = metadata["bytes"]
        ranges = handler.headers.get_all("Range", [])
        try:
            if len(ranges) > 1:
                raise ValueError("Duplicate Range")
            start, end, partial = byte_range(ranges[0] if ranges else None, size)
        except ValueError:
            body = b'{"error":"range_not_satisfiable"}'
            handler.headers_for(416, len(body), extra_headers={"Accept-Ranges": "bytes", "Content-Range": f"bytes */{size}"})
            handler.wfile.write(body)
            return
        headers = {"Accept-Ranges": "bytes"}
        if partial:
            headers["Content-Range"] = f"bytes {start}-{end}/{size}"
        handler.headers_for(206 if partial else 200, end - start + 1, metadata["content_type"], metadata["name"], extra_headers=headers)
        stream.seek(start)
        remaining = end - start + 1
        while remaining:
            if not current(store, project_id, principal):
                handler.close_connection = True
                return
            block = stream.read(min(65536, remaining))
            if not block:
                handler.close_connection = True
                return
            handler.wfile.write(block)
            remaining -= len(block)
