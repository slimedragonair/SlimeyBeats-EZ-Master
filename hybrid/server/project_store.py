"""Owner-isolated projects, shared capacity, uploads, and confirmed cleanup."""

import secrets
import time

from .errors import ApiError, ProcessingError
from .guest_sessions import limited
from .principals import OWNER
from .project_models import Project, Upload
from .project_limits import MAX_UPLOADS, CHUNK_BYTES
from .project_requests import upload_request
from .storage import directory_bytes, is_link


class ProjectStore:
    def __init__(self, jobs):
        self.jobs = jobs
        self.config, self.storage = jobs.config, jobs.storage
        self.lock = jobs.lock
        self.projects = {}

    def get(self, project_id, principal=OWNER):
        self.jobs.sessions.validate(principal)
        project = self.projects.get(project_id)
        if (project is None or project.deadline <= time.monotonic() or project.deleting
                or not principal.is_owner and project.principal_id != principal.id):
            raise ApiError(404, "project_not_found")
        return project

    def snapshot(self, project_id, principal=OWNER):
        with self.lock:
            return self.get(project_id, principal).snapshot()

    def create(self, value, principal=OWNER):
        if value != {}:
            raise ApiError(400, "invalid_fields")
        with self.lock:
            self.jobs.sessions.validate(principal)
            if self.jobs.stopping:
                raise ApiError(503, "service_stopping")
            entries = self.jobs.retained()
            owned = [entry for entry in entries if entry.principal_id == principal.id]
            if len(entries) >= self.config.max_jobs or not principal.is_owner and len(owned) >= 2:
                deadlines = [entry.deadline for entry in owned or entries]
                raise limited("project_capacity", min(deadlines), time.monotonic())
            self.storage.check_space(entries, reservation_bytes=self.config.v2_max_project_disk_bytes)
            project_id = secrets.token_hex(16)
            project = Project(project_id, principal.id, self.storage.create(project_id),
                              min(time.time() + self.config.v2_retention_seconds, principal.expires),
                              min(time.monotonic() + self.config.v2_retention_seconds, principal.deadline),
                              self.config.v2_max_project_disk_bytes)
            self.projects[project_id] = project
            return project.snapshot()

    def create_upload(self, project_id, value, principal=OWNER):
        extension, size, role = upload_request(value, self.config)
        with self.lock:
            project = self.get(project_id, principal)
            if project.command and project.command.state in ("queued", "running"):
                raise ApiError(409, "command_in_progress")
            if len(project.uploads) >= MAX_UPLOADS:
                raise ApiError(413, "upload_count_limit")
            reserved = sum(upload.total_bytes - upload.uploaded_bytes for upload in project.uploads.values())
            used = directory_bytes(project.directory, project.max_disk_bytes, project.max_storage_files)
            if used + reserved + size > project.max_disk_bytes:
                raise ApiError(413, "project_disk_limit")
            self.storage.check_space(self.jobs.retained())
            upload_id = secrets.token_hex(16)
            upload = Upload(upload_id, extension, size, role, project.directory / ("upload-" + upload_id + "." + extension))
            label = value.get("label")
            upload.source_name = label if label and label.lower().endswith("." + extension) else label + "." + extension if label else value["filename"]
            upload.kind = value.get("kind")
            project.uploads[upload_id] = upload
            return upload.snapshot()

    def upload(self, project_id, upload_id, offset, length, stream, principal=OWNER):
        with self.lock:
            project = self.get(project_id, principal)
            upload = project.uploads.get(upload_id)
            if upload is None:
                raise ApiError(404, "upload_not_found")
            project.uploads_active += 1
            project.uploads_done.clear()
        try:
            # Serialize only this upload while reading a bounded request; deletion
            # and unrelated projects must remain able to cancel a trickling client.
            with upload.lock:
                with self.lock:
                    self.get(project_id, principal)
                    if project.command and project.command.state in ("queued", "running"):
                        raise ApiError(409, "command_in_progress")
                    if offset != upload.uploaded_bytes:
                        raise ApiError(409, "upload_offset_mismatch")
                    if not 0 < length <= CHUNK_BYTES or offset + length > upload.total_bytes:
                        raise ApiError(413, "upload_limit")
                    self.storage.check_space(self.jobs.retained())
                if is_link(upload.path):
                    raise ApiError(507, "upload_unavailable")
                with upload.path.open("a+b") as output:
                    if output.tell() != offset:
                        raise ApiError(409, "upload_offset_mismatch")
                    try:
                        remaining = length
                        while remaining:
                            block = stream.read(min(65536, remaining))
                            if not block:
                                raise ApiError(400, "incomplete_upload")
                            with self.lock:
                                self.get(project_id, principal)
                            if output.write(block) != len(block):
                                raise OSError("Short write")
                            remaining -= len(block)
                        output.flush()
                        with self.lock:
                            self.get(project_id, principal)
                            upload.uploaded_bytes += length
                            return upload.snapshot()
                    except BaseException:
                        output.truncate(offset)
                        raise
        except OSError as error:
            raise ApiError(507, "upload_write_failed") from error
        finally:
            with self.lock:
                project.uploads_active -= 1
                if not project.uploads_active:
                    project.uploads_done.set()

    def cancel(self, project_id, principal=OWNER):
        with self.lock:
            project = self.get(project_id, principal)
            command = project.command
            if command and command.state in ("queued", "running"):
                command.cancel.set()
                if command.state == "queued":
                    command.state, command.error_code, command.message = "cancelled", "cancelled", "Cancelled"
                    command.done.set()
        if command and (not command.done.wait(30) or command.unreaped):
            raise ApiError(503, "cleanup_pending")
        return self.snapshot(project_id, principal)

    def delete(self, project_id, principal=OWNER, expired=False):
        with self.lock:
            project = self.projects.get(project_id) if principal.is_owner and expired else self.get_for_delete(project_id, principal)
            if project is None:
                return {"deleted": True}
            project.deleting = True
            command = project.command
            if command:
                command.cancel.set()
                if command.state == "queued":
                    command.state = "cancelled"
            arrangement = project.arrangement
        if arrangement is not None:
            arrangement.close()
        if (command and (not command.done.wait(30) or command.unreaped)
                or not project.downloads_done.wait(30) or not project.uploads_done.wait(30)):
            raise ApiError(503, "cleanup_pending")
        with self.lock:
            if self.projects.get(project_id) is project:
                try:
                    self.storage.remove(project.directory, project.id)
                except (OSError, ProcessingError) as error:
                    raise ApiError(503, "cleanup_failed") from error
                del self.projects[project_id]
        return {"deleted": True}

    def get_for_delete(self, project_id, principal):
        # A failed cleanup remains retryable even after entering deleting state.
        self.jobs.sessions.validate(principal)
        project = self.projects.get(project_id)
        if project is None or not principal.is_owner and project.principal_id != principal.id:
            raise ApiError(404, "project_not_found")
        return project

    def expire(self):
        with self.lock:
            expired = [project.id for project in self.projects.values() if project.deadline <= time.monotonic()]
        failure = None
        for project_id in expired:
            try:
                self.delete(project_id, expired=True)
            except ApiError as error:
                failure = error
        if failure:
            raise failure
