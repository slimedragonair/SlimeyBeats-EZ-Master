"""Retain exact main artifact bytes across expiry, with checked run provenance.

Upload the baseline directory as published-main-baseline with explicit retention.
Fallback trusts owned successful preview runs with an allowed branch/event pair.
"""
import hashlib
import json
import re
import stat
import tempfile
import zipfile

from pages_artifacts import (MAX_BYTES, REPOSITORY, SHA, copy_bounded, paginated,
                             select_main_artifact)

BASELINE_NAME = 'published-main-baseline'
BASELINE_EVENTS = {'current-work': {'push', 'workflow_dispatch'},
                   'main': {'workflow_run', 'workflow_dispatch'}}
METADATA_LIMIT = 16 * 1024


def archive_sha(archive):
    archive.seek(0)
    result = hashlib.file_digest(archive, 'sha256').hexdigest()
    archive.seek(0)
    return result


def write_baseline(archive, destination, run, artifact):
    for path in (destination, *destination.parents):
        if path.is_symlink() or (hasattr(path, 'is_junction') and path.is_junction()):
            raise ValueError('Baseline path must not contain symlinks or junctions')
    if destination.exists():
        raise ValueError('Baseline output must be a fresh directory')
    record = {'schema': 1, 'main_run_id': run['id'], 'main_commit': run['head_sha'],
              'main_artifact_id': artifact['id'], 'zip_sha256': archive_sha(archive)}
    destination.mkdir(parents=True)
    with (destination / 'main.zip').open('xb') as output:
        copy_bounded(archive, output)
    with (destination / 'main.json').open('x', encoding='utf-8') as output:
        json.dump(record, output, indent=2, sort_keys=True)
        output.write('\n')
    return record


def read_baseline(wrapped, destination, run, artifact):
    with zipfile.ZipFile(wrapped) as zipped:
        entries = zipped.infolist()
        if (len(entries) != 2 or {entry.filename for entry in entries} != {'main.zip', 'main.json'}
                or any(stat.S_IFMT(entry.external_attr >> 16) not in (0, stat.S_IFREG)
                       or entry.is_dir() for entry in entries)
                or sum(entry.file_size for entry in entries) > MAX_BYTES
                or zipped.getinfo('main.json').file_size > METADATA_LIMIT):
            raise ValueError('Baseline must contain only bounded regular main.zip and main.json')
        record = json.loads(zipped.read('main.json'))
        if (not isinstance(record, dict) or type(record.get('schema')) is not int
                or record['schema'] != 1 or type(record.get('main_run_id')) is not int
                or type(record.get('main_artifact_id')) is not int
                or record['main_run_id'] <= 0 or record['main_artifact_id'] <= 0
                or not isinstance(record.get('main_commit'), str)
                or not SHA.fullmatch(record['main_commit'])
                or not isinstance(record.get('zip_sha256'), str)
                or not re.fullmatch(r'[0-9a-f]{64}', record['zip_sha256'])
                or (record['main_run_id'], record['main_commit']) != (run['id'], run['head_sha'])
                or (artifact is not None and record['main_artifact_id'] != artifact['id'])):
            raise ValueError('Baseline identity does not match the latest successful main')
        with zipped.open('main.zip') as source:
            copy_bounded(source, destination)
        if archive_sha(destination) != record['zip_sha256']:
            raise ValueError('Baseline main.zip SHA-256 mismatch')
        return {'id': record['main_artifact_id'], 'name': 'github-pages',
                'expired': True, 'size_in_bytes': zipped.getinfo('main.zip').file_size}


def retained_baseline(client):
    # Main callbacks check out current-work, but GitHub labels their run branch main.
    path = 'actions/workflows/current-work-preview.yml/runs?status=success'
    for run in paginated(client, path, 'workflow_runs'):
        if (run.get('event') not in BASELINE_EVENTS.get(run.get('head_branch'), ())
                or run.get('status') != 'completed'
                or run.get('conclusion') != 'success'
                or run.get('head_repository', {}).get('full_name') != REPOSITORY):
            continue
        artifacts = [artifact for artifact in paginated(
            client, f"actions/runs/{int(run['id'])}/artifacts", 'artifacts')
            if artifact['name'] == BASELINE_NAME and not artifact['expired']]
        if len(artifacts) > 1:
            raise ValueError('Ambiguous retained main baseline')
        if artifacts:
            if not 0 < artifacts[0]['size_in_bytes'] <= MAX_BYTES:
                raise ValueError('Baseline artifact exceeds byte limit')
            return artifacts[0]
    raise ValueError('Latest successful main has expired and no retained baseline is available')


def retrieve_main(client, destination):
    run, artifact = select_main_artifact(client, allow_expired=True)
    if artifact is not None and not artifact['expired']:
        client.download(artifact['id'], destination)
    else:
        baseline = retained_baseline(client)
        with tempfile.TemporaryFile() as wrapped:
            client.download(baseline['id'], wrapped)
            if wrapped.tell() > MAX_BYTES:
                raise ValueError('Baseline download exceeds byte limit')
            wrapped.seek(0)
            artifact = read_baseline(wrapped, destination, run, artifact)
    if destination.tell() > MAX_BYTES:
        raise ValueError('Main archive exceeds byte limit')
    destination.seek(0)
    return run, artifact
