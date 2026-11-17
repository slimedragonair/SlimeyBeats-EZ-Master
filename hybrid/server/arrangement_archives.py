"""Owned opaque archive reviews; shared Rust alone parses and extracts audio."""
from dataclasses import dataclass
import json
import secrets

from .errors import ApiError, ProcessingError
from .studio_driver import safe_file
from .project_limits import MAX_ARRANGEMENT_ARCHIVE_BYTES, MAX_ARRANGEMENT_IMPORT_METADATA_BYTES
from .storage import directory_usage

MAX_PENDING_REVIEWS = 4
MAX_REVIEW_METADATA_BYTES = MAX_ARRANGEMENT_IMPORT_METADATA_BYTES


def extraction_budget(project, files, extra_entries):
    used_bytes, used_entries = directory_usage(project.directory, project.max_disk_bytes, project.max_storage_files)
    if used_entries + extra_entries + len(files) > project.max_storage_files:
        raise ApiError(413, 'project_file_limit')
    remaining = project.max_disk_bytes - used_bytes
    if sum(file['bytes'] for file in files) > remaining:
        raise ApiError(413, 'project_disk_limit')
    return remaining


@dataclass
class ArchiveReview:
    id: str
    upload_id: str
    parts: dict
    metadata_bytes: int


def checked_upload(project, upload_id):
    upload = project.uploads.get(upload_id)
    if upload is None or upload.role != 'arrangement_archive' or upload.uploaded_bytes != upload.total_bytes:
        raise ApiError(409, 'arrangement_archive_upload_incomplete')
    if upload.total_bytes > MAX_ARRANGEMENT_ARCHIVE_BYTES:
        raise ApiError(413, 'arrangement_archive_size_limit')
    return safe_file(upload.path, project.directory)


def selection(state, request):
    review = state.reviews.get(request['review_id'])
    if review is None:
        raise ApiError(404, 'arrangement_import_review_not_found')
    selected = []
    for choice in request['selection']:
        part = review.parts.get(choice['part_id'])
        version = part and part['versions'].get(choice['version_id'])
        if version is None:
            raise ApiError(400, 'arrangement_import_version_not_found')
        selected.append({'name': part['name'], 'file': version})
    return review, selected


def review_result(store, state, job, call):
    raw = call({'operation': 'inspect_archive', 'file': str(job.upload_path)})
    public, private = [], {}
    for part in raw['parts']:
        part_id = secrets.token_hex(16)
        name = ''.join(char if ord(char) >= 32 else ' ' for char in part['name']).strip() or 'Imported source'
        versions, owned = [], {}
        for file in part['versions']:
            version_id = secrets.token_hex(16)
            owned[version_id] = file
            versions.append({'id': version_id, 'format': file['format'], 'label': file['format'].upper(), 'bytes': file['bytes']})
        default = next((version['id'] for version in versions if version['format'] == 'wav'), versions[0]['id'])
        public.append({'id': part_id, 'name': name, 'versions': versions, 'default_version_id': default})
        private[part_id] = {'name': name, 'versions': owned}
    metadata_bytes = len(json.dumps(private, ensure_ascii=True).encode('utf-8'))
    review_id = secrets.token_hex(16)
    result = {'review_id': review_id, 'parts': public, 'skipped_files': raw['skipped_files'],
              'notes': ['Choose one encoding per included part. WAV is preferred; sources may have different durations. No audio has been added.']}
    with store.lock:
        if job.cancel.is_set() or state.closed or state.project.deleting:
            raise ProcessingError('Archive review cancelled')
        if len(state.reviews) >= MAX_PENDING_REVIEWS or metadata_bytes + sum(item.metadata_bytes for item in state.reviews.values()) > MAX_REVIEW_METADATA_BYTES:
            raise ApiError(429, 'arrangement_import_review_limit')
        state.reviews[review_id] = ArchiveReview(review_id, job.request['upload_id'], private, metadata_bytes)
        job.result, job.committed = result, True
    return result


def release_upload(state, upload_id):
    upload = state.project.uploads.get(upload_id)
    if upload is not None:
        path = checked_upload(state.project, upload_id)
        if path.name != 'upload-' + upload.id + '.zip':
            raise ProcessingError('Unsafe archive upload cleanup')
        try:
            path.unlink()
        except OSError as error:
            raise ApiError(507, 'arrangement_import_cleanup_pending') from error
        del state.project.uploads[upload.id]


def consume(state, review_id):
    review = state.reviews.pop(review_id)
    release_upload(state, review.upload_id)


def discard(state, review_id):
    review = state.reviews.get(review_id)
    if review is None:
        return {'discarded': False, 'review_id': review_id}
    release_upload(state, review.upload_id)
    del state.reviews[review_id]
    return {'discarded': True, 'review_id': review_id}
