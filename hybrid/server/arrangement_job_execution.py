"""Heavy work runs in a disposable shadow, leaving the live workspace intact."""
import time

from .arrangement_native import ArrangementNative, ArrangementCommandError
from .arrangement_job_artifacts import register, project_zip, remove_owned
from .eq_suggestions import suggestion
from .errors import ApiError, ProcessingError
from .storage import directory_bytes
from .studio_driver import safe_file
from .arrangement_archives import review_result, consume, extraction_budget
from .arrangement_job_adoption import adopt
import secrets


def execute(store, state, job):
    project = state.project
    deadline = time.monotonic() + state.config.worker_timeout_seconds
    promoted = False

    def check():
        if job.cancel.is_set() or project.deleting or state.closed or project.deadline <= time.monotonic() or time.monotonic() >= deadline:
            raise ProcessingError('Arrangement job cancelled or expired')
        directory_bytes(project.directory, project.max_disk_bytes, project.max_storage_files)

    def call(request):
        check()
        value = job.native.call(request, timeout=max(0.1, deadline - time.monotonic()), cancel=job.cancel, safety_check=check)
        check()
        return value

    try:
        with store.lock:
            job.state, job.progress = 'running', 0.05
        check()
        transfer = None
        if job.operation != 'review_archive':
            with state.lock:
                if state.stream is not None:
                    state.stream.close()
                    state.stream = None
                available = project.max_disk_bytes - directory_bytes(project.directory, project.max_disk_bytes, project.max_storage_files)
                transfer = state.native.call({'operation': 'fork_state', 'expected_revision': job.base_revision,
                                              'output_id': job.id, 'max_metadata_bytes': max(0, available)})
        check()
        job.progress = 0.15
        job.native = ArrangementNative(state.config, project.directory)
        doc = job.base_snapshot['arrangement']
        call(state.creation_request(doc['sample_rate'], doc['channels'], job.epoch))
        if transfer is not None:
            call({'operation': 'restore_fork', 'file': str(safe_file(transfer['file'], project.directory))})
        job.progress = 0.3
        request = job.request
        if job.operation == 'review_archive':
            result = review_result(store, state, job, call)
        elif job.operation == 'commit_import':
            selected = request['selected_files']
            available = extraction_budget(project, [item['file'] for item in selected], extra_entries=1)
            raw = call({'operation': 'extract_archive', 'file': str(job.upload_path), 'files': [item['file'] for item in selected],
                        'output_id': job.id, 'max_total_bytes': max(0, available)})
            if not isinstance(raw.get('files'), list) or len(raw['files']) != len(selected):
                raise ProcessingError('Invalid selected archive output')
            value = call({'operation': 'add_files', 'expected_revision': job.base_revision,
                'assets': [{'id': secrets.token_hex(16), 'name': item['name'], 'file': str(safe_file(file, job.stage / 'selected-sources'))}
                           for item, file in zip(selected, raw['files'])]})
            # Mark the actor adopted before cleanup can fail: it now owns the
            # retained selected-source files and must never be reaped as shadow.
            try:
                result = adopt(store, state, job, value, check)
            finally:
                promoted = state.native is job.native
            with store.lock:
                consume(state, job.review.id)
        elif job.operation == 'suggest_eq':
            raw = call({'operation': 'suggest_eq', 'expected_revision': job.base_revision, 'target': request['target'],
                        'exclude_eq_id': request['exclude_eq_id'], 'profile': request['profile']})
            duration_frames = max([job.base_snapshot['preview']['frames'], *[asset['frames'] for asset in job.base_snapshot['assets']]])
            result = suggestion(raw, {'sample_rate': doc['sample_rate'], 'duration_s': duration_frames / doc['sample_rate']})
            result['notes'].append('Measured the explicit selected scope; only its matching replaced effect was excluded. The proposal is unapplied.')
        elif job.operation == 'export':
            raw = call({'operation': 'export', 'expected_revision': job.base_revision, 'output_id': job.id,
                        'format': request['format'], 'sample_rate': request['sample_rate'], 'dither': request['dither']})
            if not isinstance(raw.get('exports'), list) or len(raw['exports']) != 1:
                raise ProcessingError('Invalid arrangement export result')
            path = safe_file(raw['exports'][0]['path'], project.directory / 'arrangement-output' / job.id)
            extension = {'mp3': 'mp3', 'flac16': 'flac', 'flac24': 'flac'}.get(request['format'], 'wav')
            mime = {'mp3': 'audio/mpeg', 'flac': 'audio/flac', 'wav': 'audio/wav'}[extension]
            artifact = register(store, project, job, path, f'Studio-arrangement.{extension}', mime)
            result = {'artifact': artifact, 'format': request['format'], 'revision': job.base_revision}
        elif job.operation == 'save_project':
            raw = call({'operation': 'save_project', 'expected_revision': job.base_revision, 'output_id': job.id})
            archive = project_zip(project, raw['file'], job.stage / 'Studio-arrangement.zip', check)
            artifact = register(store, project, job, archive, 'Studio-arrangement.zip', 'application/zip')
            result = {'artifact': artifact, 'name': artifact['name'], 'type': artifact['content_type'], 'revision': job.base_revision}
        else:
            value = call({'operation': 'open_project', 'expected_revision': job.base_revision,
                          'file': str(job.upload_path), 'epoch': request['new_epoch']})
            # Cache immutable ZIP sources in the shadow before publishing it.
            call({'operation': 'fork_state', 'expected_revision': value['revision'],
                  'output_id': request['cache_id'], 'max_metadata_bytes': 97 * 1024**2})
            try:
                result = adopt(store, state, job, value, check)
            finally:
                promoted = state.native is job.native
        with store.lock:
            if job.cancel.is_set() and not job.committed:
                raise ProcessingError('Arrangement job cancelled')
            job.result, job.state, job.progress = result, 'completed', 1.0
    except Exception as error:
        job.failure = type(error).__name__ + ': ' + str(error)
        if job.native is not None and job.native.last_error:
            job.failure += '; ' + job.native.last_error
        with store.lock:
            if job.committed and job.result is not None:
                job.state, job.progress = 'completed', 1.0
            else:
                job.state = 'cancelled' if job.cancel.is_set() or project.deleting or state.closed else 'failed'
                job.error_code = 'cancelled' if job.state == 'cancelled' else 'arrangement_job_failed'
    finally:
        try:
            if job.native is not None and not promoted:
                job.native.close()
        except (ProcessingError, OSError):
            job.unreaped=True
            job.state,job.error_code='failed','arrangement_cleanup_pending'
        try:
            if not job.keep_stage and not job.unreaped:
                remove_owned(project, job.stage, 'arrangement-job-', job.id)
            if job.operation == 'export' and not job.committed and not job.unreaped:
                remove_owned(project, project.directory / 'arrangement-output' / job.id, '', job.id)
            if job.operation == 'open_project' and not promoted and not job.unreaped:
                remove_owned(project,project.directory / ('arrangement-job-' + job.request['cache_id']),'arrangement-job-',job.request['cache_id'])
        except (ProcessingError, OSError):
            job.state,job.error_code='failed','arrangement_cleanup_pending'
        with store.jobs.changed:
            if not job.unreaped and store.jobs.arrangement_active == (project.id, job.id):
                store.jobs.arrangement_active = None
            if not job.unreaped and state.active_job is job:
                state.active_job = None
            job.done.set()
            store.jobs.changed.notify_all()
