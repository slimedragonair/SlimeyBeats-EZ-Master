"""A single processing worker joins input guards, the CLI, and artifact publication."""

import secrets
import threading

from .decoder import prepare_wav
from .errors import ProcessingError
from .native_settings import prepare_native_settings
from .processes import ProcessRunner
from .reports import sanitize_report
from .storage import directory_bytes, is_link


class Worker(threading.Thread):
    def __init__(self, config, store):
        super().__init__(name="hybrid-master-worker", daemon=False)
        self.config = config
        self.store = store
        self.runner = ProcessRunner(config)
        from .project_worker import ProjectWorker
        self.project_worker = ProjectWorker(config, store.projects)

    def run(self):
        while (job := self.store.claim()) is not None:
            from .project_models import ProjectCommand
            if isinstance(job, ProjectCommand):
                self.project_worker.execute(job)
                continue
            state, message, artifacts = "completed", "Master ready", {}
            try:
                artifacts = self.process(job)
            except Exception:
                state = "failed"
                message = "Audio processing failed; verify the input and server codec availability"
            finally:
                if job.cancel.is_set():
                    state, message = "cancelled", "Cancelled"
                if state != "completed" and not job.unreaped:
                    try:
                        # Retain just the marker and failure snapshot, no partial audio/logs.
                        self.store.storage.remove(job.directory, job.id)
                        self.store.storage.create(job.id)
                    except (OSError, ProcessingError):
                        message = "Processing stopped; temporary cleanup requires retry"
                if job.unreaped:
                    self.store.stop()
                with self.store.lock:
                    if job.cancel.is_set():
                        state, message = "cancelled", "Cancelled"
                    job.state, job.message = state, message
                    job.artifacts = artifacts if state == "completed" else {}
                    if state == "completed":
                        job.progress = 1
                    job.done.set()

    def process(self, job):
        config = self.config
        prepare_wav(job, config, self.runner)
        if job.cancel.is_set():
            raise ProcessingError("Cancelled")
        out = job.directory / "out"
        out.mkdir()
        raw_report = job.directory / "raw-report.json"
        request = job.request
        arguments, private_paths = prepare_native_settings(job, out)
        command = [config.cli, job.directory / "input.wav", *arguments,
                   "--threads", "4", "--report-json", raw_report]
        try:
            self.runner.run(command, job, config.worker_timeout_seconds)
        finally:
            # Never remove files while a child might still be using them.
            if not job.unreaped:
                for path in private_paths:
                    path.unlink()
        extension, content_type = ("mp3", "audio/mpeg") if request.format == "mp3" else ("wav", "audio/wav")
        stem = "input_master_32f" if request.format == "wav32f" else "input_master"
        audio = out / (stem + "." + extension)
        if set(out.iterdir()) != {audio} or not audio.is_file() or audio.stat().st_size <= 0 or is_link(audio):
            raise ProcessingError("Unexpected native artifacts")
        report = out / "report.json"
        sanitize_report(raw_report, report, request)
        for path in (job.directory / ("upload." + request.extension), job.directory / "input.wav", raw_report):
            path.unlink()
        directory_bytes(job.directory, config.max_job_disk_bytes)
        artifacts = {}
        for path, name, mime in ((audio, "master." + extension, content_type), (report, "report.json", "application/json")):
            artifact_id = secrets.token_hex(16)
            artifacts[artifact_id] = {"id": artifact_id, "name": name, "bytes": path.stat().st_size,
                                      "content_type": mime, "path": path}
        return artifacts
