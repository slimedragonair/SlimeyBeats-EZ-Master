"""Actual guest ZIP import count follows byte/file budgets, never an instrument cap."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import struct
import sys
import time
import wave
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from hybrid.tests.project_support import project_service, create_project, upload
from hybrid.tests.support import mint_guest
from hybrid.tools.native_fixture import frozen_worker
from hybrid.server.storage import directory_bytes


def verify(worker, ffmpeg, members):
    data, identities, lengths = io.BytesIO(), set(), []
    with zipfile.ZipFile(data, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for index in range(members):
            frames = 128 + index % 7 * 16
            audio = io.BytesIO()
            with wave.open(audio, 'wb') as writer:
                writer.setparams((1, 2, 8000, 0, 'NONE', 'not compressed'))
                writer.writeframes(struct.pack('<h', index + 1) * frames)
            archive.writestr(f'Part-{index:05}.wav', audio.getvalue())
            identities.add(hashlib.sha256(audio.getvalue()).hexdigest())
            lengths.append(frames)
    with project_service(studio_cli=worker, ffmpeg=ffmpeg, v2_arrangement_enabled=True,
                         public_enabled=True, v2_memory_bytes=2*1024**3) as (service, owner):
        client, _ = mint_guest(owner)
        project = create_project(client)
        path = f'/v2/projects/{project["id"]}/arrangement'
        status, _, state = client.request('POST', path, {'operation': 'create', 'sample_rate': 8000, 'channels': 2})
        assert status == 200, (status, state)
        private = service.store.projects.projects[project['id']]

        def job(operation, **fields):
            status, _, value = client.request('POST', path + '/jobs', {'operation': operation, 'epoch': state['epoch'],
                'expected_revision': state['revision'], **fields})
            assert status == 202, (status, value)
            deadline = time.monotonic() + 90
            while value['state'] in ('queued', 'running'):
                assert time.monotonic() < deadline, value
                time.sleep(.025)
                status, _, value = client.request('GET', path + '/jobs/' + value['job_id'])
                assert status == 200, value
            assert value['state'] == 'completed', (value, private.arrangement.jobs[value['job_id']].failure)
            return value['result']

        uploaded = upload(client, project['id'], data.getvalue(), role='arrangement_archive', filename='Parts.zip')
        review = job('review_archive', upload_id=uploaded)
        assert len(review['parts']) == members
        state = job('commit_import', review_id=review['review_id'], selection=[{
            'part_id': part['id'], 'version_id': part['default_version_id']} for part in review['parts']])['snapshot']
        assert len(state['assets']) == len(state['arrangement']['lanes']) == members
        assert state['revision'] == 1 and not private.assets, 'Arrangement sources must not consume legacy Project.assets slots'
        assert sorted(asset['frames'] for asset in state['assets']) == sorted(lengths)
        actor = private.arrangement.native
        actor.call({'operation': 'prepare', 'expected_revision': 1, 'side': 'after', 'frame': 0})
        block = actor.call({'operation': 'block', 'frames': 32})
        assert block['frames'] == 32 and len(block['samples']) == 64
        actual = {hashlib.sha256(file.read_bytes()).hexdigest() for file in private.directory.glob('arrangement-job-*/selected-sources/*.wav')}
        assert actual == identities and len(actual) == members
        storage_bytes = directory_bytes(private.directory, private.max_disk_bytes, private.max_storage_files)
        storage_entries = len(list(private.directory.rglob('*')))
        assert storage_entries <= private.max_storage_files
        for redo, lane_count in ((False, 0), (True, members)):
            status, _, state = client.request('POST', path, {'operation': 'history', 'epoch': state['epoch'],
                'expected_revision': state['revision'], 'redo': redo})
            assert status == 200 and len(state['arrangement']['lanes']) == lane_count, (status, state)
        assert client.request('DELETE', f'/v2/projects/{project["id"]}')[0] == 200
        return {'passed': True, 'principal': 'guest', 'members': members, 'legacy_project_assets': 0,
            'archive_bytes': len(data.getvalue()), 'decoded_source_bytes': sum(lengths)*4,
            'storage_bytes': storage_bytes, 'storage_entries': storage_entries,
            'storage_entry_limit': private.max_storage_files, 'checks': ['owned ZIP review above512',
                'one atomic metadata/history AddFiles', 'unequal source lengths', 'all selected raw SHA256 exact',
                'actual shared frame block', 'Undo/Redo all sources', 'existing byte/PCM/file budgets', 'owned cleanup']}


def main():
    parser = argparse.ArgumentParser()
    for name in ('worker', 'ffmpeg', 'out'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--members', type=int, default=513)
    args = parser.parse_args(); args.out.mkdir(parents=True, exist_ok=True)
    assert 0 < args.members < 32767
    worker = frozen_worker(args.worker, args.out)
    result = verify(worker, args.ffmpeg.resolve(strict=True), args.members)
    result['native_fixture'] = str(worker.parent)
    (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
