"""Allowlisted heavy operations; opaque selections never carry filesystem paths."""
from .errors import ApiError
from .project_requests import identifier
from .project_limits import FORMATS
from .profile_blends import builtin_blend

OPERATIONS = {
    'suggest_eq': {'target', 'exclude_eq_id', 'profile'},
    'export': {'format', 'sample_rate', 'dither'},
    'save_project': set(),
    'open_project': {'upload_id'},
}


def request(value, snapshot):
    if not isinstance(value, dict) or value.get('operation') not in OPERATIONS:
        raise ApiError(400, 'invalid_arrangement_job')
    operation = value['operation']
    if set(value) - (OPERATIONS[operation] | {'operation', 'epoch', 'expected_revision'}):
        raise ApiError(400, 'invalid_arrangement_job_fields')
    identifier(value.get('epoch'))
    revision = value.get('expected_revision')
    if type(revision) is not int or not 0 <= revision < 9007199254740991:
        raise ApiError(400, 'invalid_arrangement_revision')
    if value['epoch'] != snapshot['epoch'] or revision != snapshot['revision']:
        raise ApiError(409, 'arrangement_revision_stale')
    clean = dict(value)
    if operation == 'suggest_eq':
        target = value.get('target')
        if not isinstance(target, dict) or set(target) - {'scope', 'range'} or not isinstance(target.get('scope'), dict):
            raise ApiError(400, 'invalid_arrangement_target')
        scope = target['scope']
        kind = scope.get('kind')
        fields = {'mix': {'kind'}, 'lane': {'kind', 'lane_id'}, 'clip': {'kind', 'clip_id'}}
        if kind not in fields or set(scope) != fields[kind]:
            raise ApiError(400, 'invalid_arrangement_scope')
        for key in fields[kind] - {'kind'}:
            entity_id(scope[key])
        selected = target.get('range')
        if selected is not None:
            if not isinstance(selected, dict) or set(selected) != {'start_frame', 'end_frame'} or any(
                    type(frame) is not int or not 0 <= frame <= 9007199254740991 for frame in selected.values()) or selected['start_frame'] >= selected['end_frame']:
                raise ApiError(400, 'invalid_arrangement_range')
        excluded = value.get('exclude_eq_id') or None
        if excluded is not None:
            entity_id(excluded)
            saved = next((effect for effect in snapshot['arrangement']['eq'] if effect['id'] == excluded), None)
            if saved is None or saved['scope'] != scope or saved.get('range') != selected:
                raise ApiError(400, 'arrangement_eq_exclusion_mismatch')
        clean['exclude_eq_id'] = excluded
        clean['profile'] = builtin_blend(value.get('profile', 'neutral'))
    elif operation == 'export':
        if value.get('format', 'wav24') not in FORMATS:
            raise ApiError(400, 'invalid_arrangement_format')
        rate = value.get('sample_rate', 0)
        if type(rate) is not int or rate != 0 and not 8000 <= rate <= 192000:
            raise ApiError(400, 'invalid_arrangement_output_rate')
        if type(value.get('dither', False)) is not bool:
            raise ApiError(400, 'invalid_arrangement_dither')
        clean.update(format=value.get('format', 'wav24'), sample_rate=rate, dither=value.get('dither', False))
    elif operation == 'open_project':
        identifier(value.get('upload_id'))
    return clean


def entity_id(value):
    try:
        valid = isinstance(value, str) and 1 <= len(value.encode('utf-8')) <= 128 and not any(ord(char) < 32 or char in '/\\' for char in value)
    except UnicodeError:
        valid = False
    if not valid:
        raise ApiError(400, 'invalid_arrangement_entity_id')
