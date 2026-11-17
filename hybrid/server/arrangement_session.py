"""Project-owned native metadata session with explicit identity/lifetime guards."""
from copy import deepcopy
import secrets
import threading
import time

from .arrangement_native import ArrangementNative, ArrangementCommandError
from .arrangement_requests import parse
from .errors import ApiError, ProcessingError
from .project_limits import arrangement_config
from .guest_sessions import limited
from .studio_driver import safe_file
from .arrangement_preview_formats import PREVIEW_CONTAINERS, PREVIEW_MIME, preview_padding


class RemoteArrangement:
    def __init__(self, config, project):
        self.config, self.project = arrangement_config(config), project
        self.lock = threading.RLock()
        self.native = None
        self.snapshot = None
        self.source_asset_ids = set()
        self.closed = False
        self.busy = False
        self.stream = None
        self.jobs = {}
        self.expired_jobs = {}
        self.reviews = {}
        self.active_job = None
        self.cache_id = None

    def creation_request(self, rate, channels, epoch):
        return {'operation': 'create', 'sample_rate': rate, 'channels': channels, 'epoch': epoch,
                'limits': {'max_pcm_bytes': self.config.max_decoded_bytes,
                    'max_bank_pcm_bytes': min(self.config.max_bank_pcm_bytes, self.project.max_disk_bytes),
                    'max_duration_seconds': self.config.max_audio_seconds,
                    'decode_timeout_seconds': max(1, int(self.config.decode_timeout_seconds)), 'waveform_bins': 1024}}

    def create(self, request):
        with self.lock:
            if self.closed:
                raise ApiError(409, 'arrangement_closed')
            self.native = ArrangementNative(self.config, self.project.directory)
            try:
                value = self.native.call(self.creation_request(request.get('sample_rate', 48000),request.get('channels', 2),secrets.token_hex(16)))
                return self.accept(value)
            except BaseException:
                self.native.close()
                raise

    def accept(self, value):
        if not isinstance(value, dict) or not isinstance(value.get('revision'), int) or not isinstance(value.get('arrangement'), dict):
            raise ProcessingError('Invalid arrangement snapshot')
        self.snapshot = value
        return self.public()

    def public(self):
        value = deepcopy(self.snapshot)
        if value is not None:
            value['capabilities'] = {'metadata_commands': True, 'scoped_eq': True, 'waveforms': True,
                                     'playback': self.config.ffmpeg is not None, 'suggest_eq': True,
                                     'export': True, 'project': True, 'import_zip': True,
                                     'preview_containers': list(PREVIEW_CONTAINERS) if self.config.ffmpeg else []}
        return value

    def call(self, request, private_assets=None):
        with self.lock:
            if self.closed or not self.native:
                raise ApiError(409, 'arrangement_closed')
            operation = request['operation']
            if operation not in ('create', 'read', 'stop'):
                if request['epoch'] != self.snapshot['epoch'] or request['expected_revision'] != self.snapshot['revision']:
                    raise ApiError(409, 'arrangement_revision_stale')
            if operation == 'stop':
                if request['epoch'] != self.snapshot['epoch']:
                    raise ApiError(409, 'arrangement_revision_stale')
                stopped = self.stream is not None and self.stream.id == request['stream_id']
                if stopped:
                    self.stream.close()
                    self.stream = None
                return {'stopped': stopped, 'stream_id': request['stream_id']}
            if operation == 'prepare':
                try:
                    return self.prepare(request)
                except ArrangementCommandError as error:
                    raise ApiError(400, 'arrangement_command_invalid') from error
                except ProcessingError as error:
                    raise ApiError(503, 'arrangement_worker_unavailable') from error
            wire = {key: value for key, value in request.items() if key != 'epoch'}
            if operation == 'add_assets':
                wire.pop('audio_ids')
                assets, edits = [], []
                for original in private_assets:
                    source_id, lane_id, clip_id = [secrets.token_hex(16) for _ in range(3)]
                    path = safe_file(original['path'], self.project.directory)
                    assets.append({'id': source_id, 'name': original['name'], 'file': str(path)})
                    edits.append({'kind': 'add_lane', 'lane': {'id': lane_id, 'name': original['name'], 'clips': [
                        {'id': clip_id, 'source_id': source_id, 'start_frame': 0, 'source_start_frame': 0, 'end_frame': original['frames']}]}})
                wire.update(assets=assets, edits=edits)
            elif operation == 'add_uploads':
                wire = {'operation': 'add_files', 'expected_revision': request['expected_revision'],
                        'assets': [{'id': secrets.token_hex(16), 'name': name, 'file': str(safe_file(upload.path, self.project.directory))}
                                   for upload, name in private_assets]}
            if operation == 'history':
                wire.setdefault('redo', False)
            try:
                result = self.native.call(wire)
            except ArrangementCommandError as error:
                raise ApiError(400, 'arrangement_command_invalid') from error
            except ProcessingError as error:
                raise ApiError(503, 'arrangement_worker_unavailable') from error
            if operation in ('read', 'add_assets', 'add_uploads', 'edit', 'history'):
                if operation == 'add_assets':
                    self.source_asset_ids.update(request['audio_ids'])
                return self.accept(result)
            return result

    def prepare(self, request):
        from .arrangement_stream import ArrangementStream, PREVIEW_RATE, PREVIEW_PRIMING_FRAMES
        if self.config.ffmpeg is None:
            raise ApiError(503, 'arrangement_encoder_unavailable')
        if self.stream is not None:
            self.stream.close()
            self.stream = None
        result = self.native.call({'operation': 'prepare', 'expected_revision': request['expected_revision'],
                                   'side': request['side'], 'frame': request['start_frame']})
        self.accept(self.native.call({'operation': 'read'}))
        stream_id = secrets.token_hex(16)
        container = request.get('preview_container', 'mp3')
        _, padding = preview_padding(result['frames'] - result['position_frame'], result['sample_rate'])
        descriptor = {'stream_id': stream_id, 'stream_path': f'/v2/projects/{self.project.id}/arrangement/streams/{stream_id}',
            'identity': self.snapshot['preview']['identity'], 'prepared_revision': self.snapshot['revision'], 'epoch': self.snapshot['epoch'],
            'sample_rate': result['sample_rate'], 'channels': result['channels'], 'duration_frames': self.snapshot['preview']['frames'],
            'side_duration_frames': result['frames'], 'start_frame': result['position_frame'], 'side': request['side'],
            'preview_sample_rate': PREVIEW_RATE, 'preview_priming_frames': PREVIEW_PRIMING_FRAMES,
            'preview_padding_frames': padding,
            'preview_container': container, 'preview_mime': PREVIEW_MIME[container]}
        self.stream = ArrangementStream(self, descriptor)
        return descriptor

    def close(self):
        with self.lock:
            self.closed = True
            job = self.active_job
            if job is not None:
                job.cancel.set()
            if self.stream is not None:
                self.stream.close()
                self.stream = None
        if job is not None and (not job.done.wait(30) or job.unreaped):
            raise ProcessingError('Arrangement job is still stopping')
        with self.lock:
            if self.native:
                self.native.close()


def dispatch(store, project_id, request, principal):
    if not store.config.v2_arrangement_enabled or store.config.studio_cli is None:
        raise ApiError(503, 'arrangement_unavailable')
    request = parse(request)
    with store.lock:
        project = store.get(project_id, principal)
        if project.command and project.command.state in ('queued', 'running'):
            raise ApiError(409, 'command_in_progress')
        state = project.arrangement
        if request['operation'] == 'create':
            if state is not None:
                raise ApiError(409, 'arrangement_already_created')
            # Reserve under the store lock, including in-flight creation/cleanup.
            # Retained work is never evicted to admit another visitor.
            residents = [item for item in store.projects.values() if item.arrangement is not None]
            if len(residents) >= store.config.v2_arrangement_sessions:
                raise limited('arrangement_capacity', min(item.deadline for item in residents), time.monotonic())
            state = RemoteArrangement(store.config, project)
            project.arrangement = state
        elif state is None:
            raise ApiError(409, 'create_arrangement_first')
        elif state.active_job is not None and request['operation'] == 'read':
            return state.public()
        elif state.busy or state.active_job is not None and request['operation'] != 'stop':
            raise ApiError(409, 'arrangement_command_in_progress')
        if request['operation'] == 'discard_import':
            if request['epoch'] != state.snapshot['epoch'] or request['expected_revision'] != state.snapshot['revision']:
                raise ApiError(409, 'arrangement_revision_stale')
            from .arrangement_archives import discard
            return discard(state, request['review_id'])
        assets = None
        if request['operation'] == 'add_assets':
            assets = []
            for audio_id in request['audio_ids']:
                asset = project.assets.get(audio_id)
                if asset is None:
                    raise ApiError(404, 'arrangement_audio_not_found')
                assets.append(deepcopy(asset))
            # Keep the underlying immutable files while the native actor admits
            # them; failed admission releases only these provisional references.
            held = set(state.source_asset_ids)
            state.source_asset_ids.update(request['audio_ids'])
        elif request['operation'] == 'add_uploads':
            assets = []
            for index, upload_id in enumerate(request['upload_ids']):
                upload = project.uploads.get(upload_id)
                if upload is None or upload.uploaded_bytes != upload.total_bytes:
                    raise ApiError(409, 'arrangement_upload_incomplete')
                if upload.role != 'original' or upload.extension not in ('wav', 'mp3'):
                    raise ApiError(400, 'arrangement_upload_format')
                name = request['names'][index] if 'names' in request else upload.source_name.rsplit('.', 1)[0]
                assets.append((upload, name))
        state.busy = True
    try:
        if request['operation'] == 'create':
            result = state.create(request)
        else:
            result = state.call(request, assets)
        with store.lock:
            store.get(project_id, principal)
        return result
    except BaseException:
        # Keep the reserved slot until its process is confirmed stopped. A failed
        # cleanup remains owned by this project and retryable through deletion.
        if request['operation'] == 'create':
            state.close()
        with store.lock:
            if request['operation'] == 'create' and project.arrangement is state:
                project.arrangement = None
            elif request['operation'] == 'add_assets':
                state.source_asset_ids = held
        raise
    finally:
        with store.lock:
            state.busy = False
