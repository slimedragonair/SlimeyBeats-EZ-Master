"""Contained, authenticated delivery artifacts and canonical project ZIPs."""
import json
import secrets
import shutil
import zipfile
import re

from .errors import ApiError, ProcessingError
from .project_limits import MAX_ARTIFACTS
from .project_models import public_artifact
from .storage import directory_bytes, is_link
from .studio_driver import safe_file, unique, reject_constant


def register(store, project, job, file, name, mime):
    path = safe_file(file, project.directory)
    size = path.stat().st_size
    if size <= 0:
        raise ProcessingError('Empty arrangement artifact')
    directory_bytes(project.directory, project.max_disk_bytes, project.max_storage_files)
    with store.lock:
        if project.deleting or job.cancel.is_set():
            raise ProcessingError('Arrangement delivery cancelled')
        if len(project.artifacts) >= MAX_ARTIFACTS:
            raise ApiError(413, 'artifact_count_limit')
        artifact = {'id': secrets.token_hex(16), 'name': name, 'content_type': mime, 'bytes': size, 'path': path}
        project.artifacts[artifact['id']] = artifact
        job.committed = True
        if path.is_relative_to(job.stage):
            job.keep_stage = True
        return public_artifact(artifact)


def project_zip(project, manifest_file, output, check):
    manifest_file = safe_file(manifest_file, project.directory)
    if manifest_file.stat().st_size > 81 * 1024**2:
        raise ProcessingError('Canonical project manifest exceeds its budget')
    manifest = json.loads(manifest_file.read_bytes(), object_pairs_hook=unique, parse_constant=reject_constant)
    if manifest.get('format') != 'slimeybeats-studio' or manifest.get('version') != 2:
        raise ProcessingError('Unsupported canonical project output')
    names = {'arrangement.json': manifest_file}
    for source in manifest['sources']:
        name = source['file']
        if not re.fullmatch(r'assets/[a-f0-9]{64}\.wav', name):
            raise ProcessingError('Noncanonical project source member')
        names[name] = safe_file(manifest_file.parent / name, project.directory)
    with output.open('xb') as target, zipfile.ZipFile(target, 'w', compression=zipfile.ZIP_STORED, allowZip64=True) as archive:
        for name, path in names.items():
            check()
            with path.open('rb') as source, archive.open(name, 'w', force_zip64=True) as entry:
                while block := source.read(65536):
                    check()
                    entry.write(block)
    check()
    return output


def remove_owned(project, path, prefix, job_id):
    if not path.exists():
        return
    root = project.directory.resolve(strict=True)
    actual = path.resolve(strict=True)
    expected = root / (prefix + job_id) if prefix else root / 'arrangement-output' / job_id
    if actual != expected or is_link(path):
        raise ProcessingError('Unsafe arrangement cleanup path')
    entries = list(path.rglob('*'))
    if len(entries) > project.max_storage_files or any(is_link(item) for item in entries):
        raise ProcessingError('Unsafe arrangement cleanup entries')
    shutil.rmtree(path)
