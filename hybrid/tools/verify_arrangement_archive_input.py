"""Opt-in real archive qualification; originals are read only, reports private."""
import argparse
import hashlib
import json
from pathlib import Path
import secrets
import sys
import time
import wave
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from hybrid.tests.project_support import project_service, create_project
from hybrid.tests.support import mint_guest
from hybrid.tools.native_fixture import frozen_worker
from verify_arrangement_archives import upload_file


def digest(file):
    with file.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def verify(worker, ffmpeg, archive, reference, guest=False):
    identities = {file: digest(file) for file in (archive, reference)}
    with wave.open(str(reference)) as source:
        rate, channels = source.getframerate(), source.getnchannels()
    with project_service(studio_cli=worker, ffmpeg=ffmpeg, v2_arrangement_enabled=True, public_enabled=guest,
                         v2_memory_bytes=2*1024**3, v2_max_upload_bytes=512*1024**2,
                         v2_max_decoded_bytes=512*1024**2, v2_max_project_disk_bytes=1024**3) as (service, client):
        if guest:
            client, _ = mint_guest(client)
        project = create_project(client)
        path = f'/v2/projects/{project["id"]}/arrangement'
        status, _, state = client.request('POST', path, {'operation': 'create', 'sample_rate': rate, 'channels': channels})
        assert status == 200, (status, state)
        private = service.store.projects.projects[project['id']].arrangement
        uploaded = upload_file(client, project['id'], archive)

        def job(operation, **fields):
            status, _, value = client.request('POST', path + '/jobs', {'operation': operation, 'epoch': state['epoch'],
                    'expected_revision': state['revision'], **fields})
            assert status == 202, (status, value)
            deadline = time.monotonic() + 120
            while value['state'] in ('queued', 'running'):
                assert time.monotonic() < deadline, value
                time.sleep(.05)
                status, _, value = client.request('GET', path + '/jobs/' + value['job_id'])
                assert status == 200, (status, value)
            assert value['state'] == 'completed', (value, private.jobs[value['job_id']].failure)
            return value['result']

        review = job('review_archive', upload_id=uploaded)
        assert review['parts'] and not state['assets']
        chosen = review['parts'][0]
        selected = private.reviews[review['review_id']].parts[chosen['id']]['versions'][chosen['default_version_id']]
        assert selected['format'] == 'wav'
        with zipfile.ZipFile(archive) as source, source.open(selected['name']) as member:
            expected_hash = hashlib.file_digest(member, 'sha256').hexdigest()
        state = job('commit_import', review_id=review['review_id'], selection=[{
            'part_id': chosen['id'], 'version_id': chosen['default_version_id']}])['snapshot']
        assert len(state['assets']) == len(state['arrangement']['lanes']) == 1
        assert state['revision'] == 1 and uploaded not in private.project.uploads
        transfer = private.native.call({'operation': 'fork_state', 'expected_revision': 1,
                                        'output_id': secrets.token_hex(16), 'max_metadata_bytes': 1024**2})
        sources = json.loads(Path(transfer['file']).read_text(encoding='utf-8'))['sources']
        assert len(sources) == 1 and sources[0]['identity']['sha256'] == expected_hash
        assert digest(Path(sources[0]['file'])) == expected_hash
        private.native.call({'operation': 'prepare', 'expected_revision': 1, 'side': 'after', 'frame': 0})
        block = private.native.call({'operation': 'block', 'frames': 2048})
        assert block['frames'] == 2048 and len(block['samples']) == 2048 * channels
        assert all(isinstance(value, (int, float)) and abs(value) < 2 for value in block['samples'])
        status, _, undone = client.request('POST', path, {'operation': 'history', 'epoch': state['epoch'], 'expected_revision': 1, 'redo': False})
        assert status == 200 and not undone['arrangement']['lanes']
        status, _, restored = client.request('POST', path, {'operation': 'history', 'epoch': state['epoch'], 'expected_revision': undone['revision'], 'redo': True})
        assert status == 200 and len(restored['arrangement']['lanes']) == 1
        assert client.request('DELETE', f'/v2/projects/{project["id"]}')[0] == 200
    assert all(digest(file) == identity for file, identity in identities.items())
    return {'passed': True, 'principal': 'guest' if guest else 'owner', 'archive_bytes': archive.stat().st_size, 'archive_sha256': identities[archive],
            'reference_sha256': identities[reference], 'reviewed_parts': len(review['parts']),
            'reviewed_versions': sum(len(part['versions']) for part in review['parts']), 'selected_sources': 1,
            'sample_rate': rate, 'channels': channels, 'selected_frames': sources[0]['frames'],
            'checks': ['real ZIP chunk upload', 'server shared review', 'single-part AddFiles without aligned-bank requirement',
                       'selected original member SHA256 exact', 'actual frame block', 'one Undo/Redo',
                       'owned cleanup', 'both original hashes unchanged'],
            'scope': 'one selected WAV source; all-part decode and browser/phone playback not qualified by this probe'}


def main():
    parser = argparse.ArgumentParser()
    for name in ('worker', 'ffmpeg', 'archive', 'reference', 'out'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--guest', action='store_true')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    worker = frozen_worker(args.worker, args.out)
    result = verify(worker, args.ffmpeg.resolve(strict=True), args.archive.resolve(strict=True), args.reference.resolve(strict=True), args.guest)
    result['native_fixture'] = str(worker.parent)
    (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
