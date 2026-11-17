"""One guarded primary-actor handoff for project Open and atomic source import."""
from .arrangement_job_artifacts import remove_owned
from .errors import ProcessingError


def adopt(store, state, job, value, check):
    with state.lock, store.lock:
        check()
        if state.snapshot['epoch'] != job.epoch or state.snapshot['revision'] != job.base_revision:
            raise ProcessingError('Arrangement changed before adoption')
        previous, previous_cache = state.native, state.cache_id
        state.native = job.native
        if job.operation == 'open_project':
            state.cache_id = job.request['cache_id']
            state.source_asset_ids.clear()
        state.accept(value)
        job.committed, job.keep_stage = True, True
        job.result = {'snapshot': state.public()}
    try:
        previous.close()
    except ProcessingError:
        job.unreaped = True
        raise
    # Imported sources can still refer to earlier immutable caches. Only a
    # complete project replacement makes this previous pool disposable.
    if job.operation == 'open_project' and previous_cache is not None:
        remove_owned(state.project, state.project.directory / ('arrangement-job-' + previous_cache),
                     'arrangement-job-', previous_cache)
    return job.result
