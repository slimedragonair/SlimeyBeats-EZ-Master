"""A single processing worker joins input guards, the CLI, and artifact publication."""

import secrets
import threading

from .decoder import prepare_wav
from .errors import ProcessingError
from .processes import ProcessRunner
from .reports import sanitize_report
from .storage import directory_bytes, is_link


class Worker(threading.Thread):
    def __init__(self, config, store):
        super().__init__(name="hybrid-master-worker", daemon=False)
        self.config = config
        self.store = store
        self.runner = ProcessRunner(config)

    def run(self):
        while (job := self.store.claim()) is not None:
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
        command = [config.cli, job.directory / "input.wav", "--out", out,
                   "--format", request.format, "--profile", request.profile, "--mode", request.mode,
                   "--ceiling", str(request.ceiling_dbtp), "--intensity", str(request.intensity),
                   "--threads", "4", "--report-json", raw_report]
        if request.target_lufs is not None:
            command.extend(["--lufs", str(request.target_lufs)])
        self.runner.run(command, job, config.worker_timeout_seconds)
        extension, content_type = ("wav", "audio/wav") if request.format == "wav24" else ("mp3", "audio/mpeg")
        audio = out / ("input_master." + extension)
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
