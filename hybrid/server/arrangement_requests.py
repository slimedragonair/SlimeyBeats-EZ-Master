"""Small public metadata contract; shared Rust validates nested edits/scopes."""
from .errors import ApiError
from .project_requests import identifier

FIELDS = {
    'create': {'sample_rate', 'channels'},
    'read': set(),
    'add_assets': {'audio_ids'},
    'add_uploads': {'upload_ids', 'names'},
    'edit': {'edits'},
    'history': {'redo'},
    'waveform': {'asset_id', 'range', 'bins'},
    'eq_response': {'effect'},
    'prepare': {'side', 'start_frame'},
    'stop': {'stream_id'},
}


def parse(value):
    if not isinstance(value, dict) or value.get('operation') not in FIELDS:
        raise ApiError(400, 'invalid_arrangement_operation')
    operation = value['operation']
    allowed = FIELDS[operation] | {'operation'}
    if operation not in ('create', 'read'):
        allowed |= {'epoch', 'expected_revision'}
        identifier(value.get('epoch'))
        revision = value.get('expected_revision')
        if type(revision) is not int or not 0 <= revision < 9007199254740991:
            raise ApiError(400, 'invalid_arrangement_revision')
    if set(value) - allowed:
        raise ApiError(400, 'invalid_arrangement_fields')
    if operation == 'create':
        if type(value.get('sample_rate', 48000)) is not int or not 8000 <= value.get('sample_rate', 48000) <= 384000:
            raise ApiError(400, 'invalid_arrangement_rate')
        if type(value.get('channels', 2)) is not int or value.get('channels', 2) not in (1, 2):
            raise ApiError(400, 'invalid_arrangement_channels')
    elif operation in ('add_assets', 'add_uploads'):
        key = 'audio_ids' if operation == 'add_assets' else 'upload_ids'
        if not isinstance(value.get(key), list) or not value[key]:
            raise ApiError(400, 'arrangement_audio_ids_required')
        if len(set(map(str, value[key]))) != len(value[key]):
            raise ApiError(400, 'duplicate_arrangement_audio_id')
        for audio_id in value[key]:
            identifier(audio_id)
        if 'names' in value:
            names = value['names']
            if not isinstance(names, list) or len(names) != len(value[key]):
                raise ApiError(400, 'invalid_arrangement_names')
            for name in names:
                try:
                    valid = isinstance(name, str) and bool(name.strip()) and len(name.encode('utf-8')) <= 512 and all(ord(char) >= 32 for char in name)
                except UnicodeError:
                    valid = False
                if not valid:
                    raise ApiError(400, 'invalid_arrangement_names')
    elif operation == 'edit':
        if not isinstance(value.get('edits'), list) or not all(isinstance(edit, dict) for edit in value['edits']):
            raise ApiError(400, 'invalid_arrangement_edits')
    elif operation == 'history':
        if type(value.get('redo', False)) is not bool:
            raise ApiError(400, 'invalid_arrangement_history')
    elif operation == 'waveform':
        identifier(value.get('asset_id'))
        if type(value.get('bins')) is not int or not 1 <= value['bins'] <= 16384 or not isinstance(value.get('range'), dict):
            raise ApiError(400, 'invalid_arrangement_waveform')
    elif operation == 'eq_response' and not isinstance(value.get('effect'), dict):
        raise ApiError(400, 'invalid_arrangement_effect')
    elif operation == 'prepare':
        if value.get('side') not in ('before', 'after') or type(value.get('start_frame')) is not int or not 0 <= value['start_frame'] <= 9007199254740991:
            raise ApiError(400, 'invalid_arrangement_prepare')
    elif operation == 'stop':
        identifier(value.get('stream_id'))
    return dict(value)
