"""Real server-side ZIP review/admission; large uploads never decode on a phone."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import secrets
import struct
import subprocess
import sys
import tempfile
import threading
import time
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from hybrid.tests.project_support import project_service, create_project, upload
from hybrid.tests.support import mint_guest
from hybrid.server.arrangement_job_adoption import adopt
from hybrid.tools.native_fixture import frozen_worker
from verify_arrangement_http import fixture


def upload_file(client, project_id, file):
    base = f'/v2/projects/{project_id}/uploads'
    status, _, value = client.request('POST', base, {'filename': 'Suno.zip', 'bytes': file.stat().st_size, 'role': 'arrangement_archive'})
    assert status == 201, (status, value)
    with file.open('rb') as source:
        offset = 0
        while block := source.read(1024**2):
            status, _, result = client.request('PUT', base + '/' + value['id'] + '?offset=' + str(offset), raw=block)
            assert status == 200, (status, result)
            offset += len(block)
    return value['id']


def verify(worker, ffmpeg, directory, large_mib):
    inputs = {'Voice': fixture(4000), 'Backing': fixture(6000)}
    mp3 = subprocess.run([str(ffmpeg), '-nostdin', '-v', 'error', '-i', 'pipe:0', '-f', 'mp3', 'pipe:1'],
                         input=inputs['Voice'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=15).stdout
    archive_path = directory / 'Suno.zip'
    with zipfile.ZipFile(archive_path, 'w', compression=zipfile.ZIP_STORED) as archive:
        for name, audio in inputs.items():
            archive.writestr('WAV/' + name + '.wav', audio)
        archive.writestr('MP3/Voice.mp3', mp3)
        if large_mib:
            with archive.open('unselected-notes.bin', 'w') as filler:
                for _ in range(large_mib):
                    filler.write(bytes(1024**2))
    with project_service(studio_cli=worker, ffmpeg=ffmpeg, v2_arrangement_enabled=True, public_enabled=True,
                         v2_memory_bytes=2*1024**3, v2_max_upload_bytes=512*1024**2,
                         v2_max_project_disk_bytes=768*1024**2) as (service, client):
        project = create_project(client)
        path = f'/v2/projects/{project["id"]}/arrangement'
        status, _, state = client.request('POST', path, {'operation': 'create', 'sample_rate': 8000, 'channels': 2})
        assert status == 200, (status, state)
        private = service.store.projects.projects[project['id']].arrangement
        primary = private.native
        uploaded = upload_file(client, project['id'], archive_path)

        def submit(operation, **fields):
            status, _, job = client.request('POST', path + '/jobs', {'operation': operation, 'epoch': state['epoch'],
                        'expected_revision': state['revision'], **fields})
            assert status == 202, (status, job)
            return job

        def finish(job, expected='completed'):
            deadline = time.monotonic() + 40
            while job['state'] in ('queued', 'running'):
                assert time.monotonic() < deadline, job
                time.sleep(.02)
                status, _, job = client.request('GET', path + '/jobs/' + job['job_id'])
                assert status == 200, (status, job)
            assert job['state'] == expected, (job, private.jobs[job['job_id']].failure)
            return job.get('result')

        job = submit('review_archive', upload_id=uploaded)
        review = finish(job)
        assert len(review['parts']) == 2 and private.snapshot['revision'] == 0
        assert '"file":' not in json.dumps(review) and str(directory) not in json.dumps(review)
        voice = next(part for part in review['parts'] if part['name'] == 'Voice')
        assert {item['format'] for item in voice['versions']} == {'wav', 'mp3'}
        assert next(item for item in voice['versions'] if item['id'] == voice['default_version_id'])['format'] == 'wav'
        selection = [{'part_id': part['id'], 'version_id': part['default_version_id']} for part in review['parts']]
        request = {'operation': 'commit_import', 'epoch': state['epoch'], 'expected_revision': 0,
                   'review_id': review['review_id'], 'selection': selection}
        status, _, _ = client.request('POST', path + '/jobs', dict(request, selection=selection + [selection[0]]))
        assert status == 400
        invalid = [dict(selection[0], version_id=secrets.token_hex(16))]
        assert client.request('POST', path + '/jobs', dict(request, selection=invalid))[0] == 400
        guest, _ = mint_guest(client)
        assert guest.request('GET', path + '/jobs/' + job['job_id'])[0] == 404
        assert guest.request('POST', path + '/jobs', request)[0] == 404

        entered, release = threading.Event(), threading.Event()
        shadows = []
        def held(store, current, job, value, check):
            shadows.append(job.native)
            assert len(value['assets']) == 2 and (job.stage / 'selected-sources').is_dir()
            entered.set()
            assert release.wait(10)
            return adopt(store, current, job, value, check)
        # Hold after actual selected extraction, decode and AddFiles. Cancel
        # must still prevent adoption, rather than merely skip a queued job.
        with patch('hybrid.server.arrangement_job_execution.adopt', held):
            cancelled = submit('commit_import', review_id=review['review_id'], selection=selection)
            try:
                assert entered.wait(10)
                assert client.request('POST', path + '/jobs/' + cancelled['job_id'] + '/cancel', {})[0] == 200
            finally:
                release.set()
            finish(cancelled, 'cancelled')
        assert shadows[0].process.poll() is not None and private.native is primary and private.snapshot['revision'] == 0
        assert not private.jobs[cancelled['job_id']].stage.exists()
        assert review['review_id'] in private.reviews
        opened = finish(submit('commit_import', review_id=review['review_id'], selection=selection))['snapshot']
        assert opened['epoch'] == state['epoch'] and opened['revision'] == 1
        assert {lane['name']: lane['clips'][0]['end_frame'] for lane in opened['arrangement']['lanes']} == {'Voice': 4000, 'Backing': 6000}
        assert len(opened['assets']) == 2 and primary.process.poll() is not None
        assert review['review_id'] not in private.reviews and uploaded not in private.project.uploads
        state = opened
        # The retained raw selected sources, not a padded/aligned bank, are the
        # exact archive members. Export/save can reuse them after review expiry.
        transfer = private.native.call({'operation': 'fork_state', 'expected_revision': 1,
                                        'output_id': secrets.token_hex(16), 'max_metadata_bytes': 1024**2})
        sources = json.loads(Path(transfer['file']).read_text(encoding='utf-8'))['sources']
        for source in sources:
            assert source['identity']['sha256'] == hashlib.sha256(inputs[source['name']]).hexdigest()
        status, _, undone = client.request('POST', path, {'operation': 'history', 'epoch': state['epoch'], 'expected_revision': 1, 'redo': False})
        assert status == 200 and not undone['arrangement']['lanes']
        status, _, state = client.request('POST', path, {'operation': 'history', 'epoch': state['epoch'], 'expected_revision': undone['revision'], 'redo': True})
        assert status == 200 and len(state['arrangement']['lanes']) == 2
        private.native.call({'operation': 'prepare', 'expected_revision': state['revision'], 'side': 'after', 'frame': 0})
        block = private.native.call({'operation': 'block', 'frames': 512})
        assert block['frames'] == 512 and max(map(abs, block['samples'])) > .05

        # Bad paths and CRC changes stay private and preserve the current actor.
        for bad_name in ('../Escape.wav', 'C:/Escape.wav'):
            data = io.BytesIO()
            with zipfile.ZipFile(data, 'w') as archive:
                archive.writestr(bad_name, inputs['Voice'])
            bad = upload(client, project['id'], data.getvalue(), role='arrangement_archive', filename='Unsafe.zip')
            finish(submit('review_archive', upload_id=bad), 'failed')
        assert private.snapshot['revision'] == state['revision']
        single = io.BytesIO()
        with zipfile.ZipFile(single, 'w') as archive:
            archive.writestr('Only.wav', inputs['Voice'])
        single_id = upload(client, project['id'], single.getvalue(), role='arrangement_archive', filename='Single.zip')
        pending = finish(submit('review_archive', upload_id=single_id))
        assert len(pending['parts']) == 1
        owned_zip = private.project.uploads[single_id].path
        with owned_zip.open('r+b') as changed:
            changed.seek(30 + len('Only.wav') + 70)
            old = changed.read(1)
            changed.seek(-1, 1)
            changed.write(bytes([old[0] ^ 1]))
        saved_native = private.native
        rejected = submit('commit_import', review_id=pending['review_id'], selection=[{
            'part_id': pending['parts'][0]['id'], 'version_id': pending['parts'][0]['default_version_id']}])
        finish(rejected, 'failed')
        assert private.native is saved_native and private.snapshot['revision'] == state['revision']
        assert not private.jobs[rejected['job_id']].stage.exists()
        assert pending['review_id'] in private.reviews
        discard = {'operation': 'discard_import', 'epoch': state['epoch'], 'expected_revision': state['revision'], 'review_id': pending['review_id']}
        assert guest.request('POST', path, discard)[0] == 404
        assert client.request('POST', path, discard)[2]['discarded'] is True
        assert single_id not in private.project.uploads
        assert client.request('POST', path, discard)[2]['discarded'] is False
        alternative = io.BytesIO()
        with zipfile.ZipFile(alternative, 'w') as archive:
            archive.writestr('WAV/Voice.wav', inputs['Voice'])
            archive.writestr('MP3/Voice.mp3', mp3)
        alternate_id = upload(client, project['id'], alternative.getvalue(), role='arrangement_archive', filename='Versions.zip')
        alternate = finish(submit('review_archive', upload_id=alternate_id))
        part = alternate['parts'][0]
        mp3_id = next(version['id'] for version in part['versions'] if version['format'] == 'mp3')
        state = finish(submit('commit_import', review_id=alternate['review_id'], selection=[{'part_id': part['id'], 'version_id': mp3_id}]))['snapshot']
        assert len(state['assets']) == 3 and len(state['arrangement']['lanes']) == 3
        transfer = private.native.call({'operation': 'fork_state', 'expected_revision': state['revision'],
                                        'output_id': secrets.token_hex(16), 'max_metadata_bytes': 1024**2})
        retained_sources = json.loads(Path(transfer['file']).read_text(encoding='utf-8'))['sources']
        assert {source['identity']['sha256'] for source in retained_sources} == {
            *[hashlib.sha256(value).hexdigest() for value in inputs.values()], hashlib.sha256(mp3).hexdigest()}
        assert all(Path(source['file']).is_file() for source in retained_sources)
        # One valid member must not leak into the primary when a second decoded
        # member has the wrong project rate. This exercises batch atomicity.
        mixed_rates = io.BytesIO()
        other_rate = bytearray(inputs['Backing'])
        struct.pack_into('<I', other_rate, 24, 16000)
        struct.pack_into('<I', other_rate, 28, 32000)
        with zipfile.ZipFile(mixed_rates, 'w') as archive:
            archive.writestr('A-valid.wav', inputs['Voice'])
            archive.writestr('Z-other-rate.wav', other_rate)
        bad_rate_id = upload(client, project['id'], mixed_rates.getvalue(), role='arrangement_archive', filename='Rates.zip')
        invalid_review = finish(submit('review_archive', upload_id=bad_rate_id))
        unchanged = json.dumps(private.snapshot, sort_keys=True)
        saved_native = private.native
        rejected = submit('commit_import', review_id=invalid_review['review_id'], selection=[{
            'part_id': part['id'], 'version_id': part['default_version_id']} for part in invalid_review['parts']])
        finish(rejected, 'failed')
        assert private.native is saved_native and json.dumps(private.snapshot, sort_keys=True) == unchanged
        assert invalid_review['review_id'] in private.reviews and not private.jobs[rejected['job_id']].stage.exists()
        assert client.request('POST', path, {'operation': 'discard_import', 'epoch': state['epoch'],
            'expected_revision': state['revision'], 'review_id': invalid_review['review_id']})[2]['discarded'] is True
        assert client.request('DELETE', f'/v2/projects/{project["id"]}')[0] == 200
        return {'passed': True, 'synthetic_archive_bytes': archive_path.stat().st_size,
                'scope': 'actual server upload/native ZIP/PCM; not the user archive or phone acceptance',
                'checks': ['chunked original ZIP upload', 'shared WAV/MP3 review and WAV default', 'owned opaque IDs',
                    'one encoding per part and explicit MP3 choice', 'cancel after actual decode prevents adoption and reaps shadow',
                    'one atomic unequal-duration AddFiles', 'wrong-rate batch preserves primary without partial import',
                    'exact selected source SHA256', 'one Undo/Redo', 'actual frame PCM', 'unsafe paths rejected',
                    'single-source review', 'changed ZIP CRC rejection and contained cleanup',
                    'foreign ownership', 'idempotent discard/upload cleanup']}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--worker', type=Path, required=True)
    parser.add_argument('--ffmpeg', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--large-mib', type=int, default=0, choices=range(0, 513))
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    worker = frozen_worker(args.worker, args.out)
    with tempfile.TemporaryDirectory(prefix='archive-input-', dir=args.out) as temp:
        result = verify(worker, args.ffmpeg.resolve(strict=True), Path(temp), args.large_mib)
    result['native_fixture'] = str(worker.parent)
    (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
