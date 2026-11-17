"""Allowlisted heavy operations; opaque selections never carry filesystem paths."""
import json

from .errors import ApiError
from .project_requests import identifier
from .project_limits import FORMATS, MAX_ARRANGEMENT_IMPORT_METADATA_BYTES
from .profile_blends import builtin_blend
from .arrangement_ids import entity_id

OPERATIONS = {
    'suggest_eq': {'target', 'exclude_eq_id', 'profile'},
    'export': {'format', 'sample_rate', 'dither'},
    'save_project': set(),
    'open_project': {'upload_id'},
    'review_archive': {'upload_id'},
    'commit_import': {'review_id', 'selection'},
}


def request(value, snapshot):
    operation = value.get('operation') if isinstance(value, dict) else None
    if not isinstance(operation, str) or operation not in OPERATIONS:
        raise ApiError(400, 'invalid_arrangement_job')
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
        if not isinstance(kind, str) or kind not in fields or set(scope) != fields[kind]:
            raise ApiError(400, 'invalid_arrangement_scope')
        for key in fields[kind] - {'kind'}:
            entity_id(scope[key])
        selected = target.get('range')
        if selected is not None:
            if not isinstance(selected, dict) or set(selected) != {'start_frame', 'end_frame'} or any(
                    type(frame) is not int or not 0 <= frame <= 9007199254740991 for frame in selected.values()) or selected['start_frame'] >= selected['end_frame']:
                raise ApiError(400, 'invalid_arrangement_range')
        excluded = value.get('exclude_eq_id')
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
    elif operation in ('open_project', 'review_archive'):
        identifier(value.get('upload_id'))
    elif operation == 'commit_import':
        identifier(value.get('review_id'))
        choices = value.get('selection')
        if not isinstance(choices, list) or not choices:
            raise ApiError(400, 'invalid_arrangement_import_selection')
        seen = set()
        for choice in choices:
            if not isinstance(choice, dict) or set(choice) != {'part_id', 'version_id'}:
                raise ApiError(400, 'invalid_arrangement_import_selection')
            identifier(choice['part_id'])
            identifier(choice['version_id'])
            if choice['part_id'] in seen:
                raise ApiError(400, 'duplicate_arrangement_import_part')
            seen.add(choice['part_id'])
        if len(json.dumps(choices, separators=(',', ':')).encode('utf-8')) > MAX_ARRANGEMENT_IMPORT_METADATA_BYTES:
            raise ApiError(413, 'arrangement_import_metadata_limit')
    return clean
