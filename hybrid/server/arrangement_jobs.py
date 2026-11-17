"""Authenticated async arrangement jobs, shared native capacity and polling."""
from copy import deepcopy
import secrets
import threading

from .arrangement_job_state import ArrangementJob, MAX_JOB_RECORDS, prune_terminal_jobs
from .arrangement_job_requests import request as job_request
from .arrangement_job_execution import execute
from .client_identity import client_key
from .errors import ApiError
from .project_requests import identifier
from .studio_driver import safe_file


def selected(store, project_id, principal):
    project = store.get(project_id, principal)
    if not store.config.v2_arrangement_enabled or project.arrangement is None:
        raise ApiError(409, 'create_arrangement_first')
    return project, project.arrangement


def submit(store, project_id, value, principal, client_identity=None):
    with store.jobs.changed:
        project, state = selected(store, project_id, principal)
        request = job_request(value, state.snapshot)
        if store.jobs.stopping:
            raise ApiError(503, 'service_stopping')
        if state.busy or state.active_job is not None or store.jobs.arrangement_active is not None:
            raise ApiError(409, 'arrangement_job_in_progress')
        if store.jobs.queue or any(item.state == 'running' for item in store.jobs.jobs.values()) or any(
                item.command and item.command.state in ('queued', 'running') for item in store.projects.values()):
            raise ApiError(409, 'native_worker_busy')
        prune_terminal_jobs(state)
        if len(state.jobs) >= MAX_JOB_RECORDS:
            raise ApiError(429, 'arrangement_job_history_limit')
        upload_path = None
        if request['operation'] == 'open_project':
            upload = project.uploads.get(request['upload_id'])
            if upload is None or upload.role != 'arrangement_project' or upload.uploaded_bytes != upload.total_bytes:
                raise ApiError(409, 'arrangement_project_upload_incomplete')
            upload_path = safe_file(upload.path, project.directory)
            request.update(new_epoch=secrets.token_hex(16), cache_id=secrets.token_hex(16))
        if not principal.is_owner:
            if client_identity is None:
                raise ApiError(400, 'guest_client_identity_required')
            store.jobs.sessions.start_job(principal, client_identity)
        job_id = secrets.token_hex(16)
        job = ArrangementJob(job_id, request['operation'], state.snapshot['epoch'], state.snapshot['revision'],
                             project.directory / ('arrangement-job-' + job_id), request, deepcopy(state.snapshot))
        job.upload_path = upload_path
        state.jobs[job_id] = state.active_job = job
        store.jobs.arrangement_active = (project.id, job_id)
        job.thread = threading.Thread(target=execute, args=(store, state, job), name='arrangement-job', daemon=False)
        try:
            job.thread.start()
        except BaseException:
            state.jobs.pop(job_id, None)
            state.active_job = None
            store.jobs.arrangement_active = None
            raise
        return job.public()


def get(store, project_id, job_id, principal, cancel=False):
    identifier(job_id)
    with store.lock:
        _, state = selected(store, project_id, principal)
        job = state.jobs.get(job_id)
        if job is None:
            if job_id in state.expired_jobs:
                raise ApiError(410, 'arrangement_job_expired')
            raise ApiError(404, 'arrangement_job_not_found')
        if cancel and job.state in ('queued', 'running') and not job.committed:
            job.cancel.set()
        return job.public()
