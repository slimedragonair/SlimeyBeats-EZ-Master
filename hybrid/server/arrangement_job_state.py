"""Bounded task records and owned cancellation, separate from editable metadata."""
from dataclasses import dataclass, field
from copy import deepcopy
import threading

MAX_JOB_RECORDS = 32
MAX_EXPIRED_JOB_IDS = 128
TERMINAL_STATES = frozenset(('completed', 'failed', 'cancelled'))


def prune_terminal_jobs(state):
    """Expire records under the store lock; artifacts retain project ownership."""
    for job_id, job in list(state.jobs.items()):
        if len(state.jobs) < MAX_JOB_RECORDS:
            break
        # done is published only after native reaping and owned cleanup. A
        # terminal state alone can still have a live child or active lease.
        if job is state.active_job or job.unreaped or not job.done.is_set() or job.state not in TERMINAL_STATES:
            continue
        del state.jobs[job_id]
        state.expired_jobs[job_id] = None
        while len(state.expired_jobs) > MAX_EXPIRED_JOB_IDS:
            del state.expired_jobs[next(iter(state.expired_jobs))]


@dataclass
class ArrangementJob:
    id: str
    operation: str
    epoch: str
    base_revision: int
    stage: object
    request: dict
    base_snapshot: dict
    state: str = 'queued'
    progress: float = 0.0
    result: dict | None = None
    error_code: str | None = None
    committed: bool = False
    keep_stage: bool = False
    cancel: threading.Event = field(default_factory=threading.Event)
    done: threading.Event = field(default_factory=threading.Event)
    thread: object = None
    native: object = None
    upload_path: object = None
    unreaped: bool = False
    failure: str | None = field(default=None, repr=False)

    def public(self):
        terminal = self.state in TERMINAL_STATES
        published = not terminal or self.done.is_set()
        value = {'job_id': self.id, 'operation': self.operation, 'epoch': self.epoch,
                 'base_revision': self.base_revision, 'state': self.state if published else 'running', 'progress': self.progress if published else min(self.progress,0.99)}
        if self.result is not None and published:
            value['result'] = deepcopy(self.result)
        if self.error_code is not None and published:
            value['error_code'] = self.error_code
        return value
